use ast::*;
use binding_map::BindingMap;
use closure::Closure;
use compiler::*;
use grammar::*;
use instructions::*;
use native::find_lib;
use native::FileDescriptorMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use value::Value;

use exception_handler::ExceptionHandler;

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Eq, Debug, PartialEq)]
struct Frame {
    bindings: BindingMap,
    exception_handlers: Vec<ExceptionHandler>,
}

impl Frame {
    pub fn new(bindings: BindingMap) -> Frame {
        Frame {
            bindings: bindings,
            exception_handlers: Vec::new(),
        }
    }
}

#[derive(Clone, Eq, Debug, PartialEq)]
pub struct Vm {
    instructions: Rc<InstructionSequence>,
    pc: usize,
    stack: Vec<Value>,
    frames: Vec<Frame>,
    pub file_descriptors: FileDescriptorMap,
}

impl Vm {
    pub fn new(source: &str) -> Vm {
        let stmts = statements(source);
        let instructions = compile(&stmts.unwrap());
        let map = BindingMap::new(None);
        let frame = Frame::new(map);

        let vm = Vm {
            instructions: Rc::new(instructions),
            pc: 0,
            stack: Vec::new(),
            frames: vec![frame],
            file_descriptors: FileDescriptorMap::new(),
        };
        vm
    }

    pub fn empty() -> Vm {
        let map = BindingMap::new(None);
        let frame = Frame::new(map);
        let vm = Vm {
            instructions: Rc::new(vec![]),
            pc: 0,
            stack: Vec::new(),
            frames: vec![frame],
            file_descriptors: FileDescriptorMap::new(),
        };
        vm
    }

    pub fn run<'b>(&'b mut self) {
        loop {
            let insn_result = Vm::next_instruction(self);
            let instruction;

            instruction = if let Some(i) = insn_result {
                trace!("next instruction: {:?}", i);
                i
            } else {
                trace!("instruction {:?} not found, terminating", insn_result);
                break;
            };

            match instruction {
                Instruction::Clear => self.stack.clear(),
                Instruction::Push(ref value) => {
                    let top_bindings = &mut self.frames.last_mut().unwrap().bindings;
                    self.stack.push(Vm::literal_to_value(value, top_bindings))
                }
                Instruction::Assign(ref binding_name) => {
                    let value = self.stack.pop().unwrap();
                    self.frames
                        .last_mut()
                        .unwrap()
                        .bindings
                        .assign(binding_name, value)
                }
                Instruction::LocalAssign(ref binding_name) => {
                    let value = self.stack.pop().unwrap();
                    self.frames
                        .last_mut()
                        .unwrap()
                        .bindings
                        .local_assign(binding_name, value)
                }
                Instruction::Call(arg_size) => {
                    let closure_info = match self.stack.pop() {
                        Some(Value::Closure(arg_names, closure)) => Ok((arg_names, closure)),
                        Some(x) => Err(format!("expected a closure, got {:?}", x)),
                        None => Err(format!("expected a closure, got None")),
                    };

                    let new_stack_length = { self.stack.len() - arg_size };
                    let mut args = self.stack.split_off(new_stack_length);
                    let (closure_args, closure) = match closure_info {
                        Ok(info) => info,
                        Err(m) => panic!(m),
                    };
                    if arg_size != closure_args.len() {
                        panic!(
                            "wrong number of arguments, expected {:?}, got {:?}",
                            closure_args.len(),
                            arg_size
                        )
                    };
                    let local_bindings = (*closure_args)
                        .clone()
                        .into_iter()
                        .rev()
                        .map(|arg_name| (arg_name, args.pop().unwrap()))
                        .collect();

                    self.reset_instructions(
                        closure.instructions.clone(),
                        Some(closure.init_map(local_bindings)),
                    );
                }
                Instruction::Fetch(ref binding_name) => {
                    let value = self.fetch(binding_name).unwrap();
                    self.stack.push(value);
                }
                Instruction::MakeMap(size) => {
                    let map = (0..size)
                        .into_iter()
                        .map(|_| {
                            let value = self.stack.pop().unwrap();
                            let key = self.stack.pop().unwrap();
                            (key, value)
                        })
                        .collect();
                    self.stack.push(Value::Map(Rc::new(RefCell::new(map))))
                }
                Instruction::Rescue(ref pattern, ref iseq) => {
                    let top_bindings = { &mut self.frames.last_mut().unwrap().bindings.clone() };
                    let closure = Closure::new(iseq.clone(), top_bindings);
                    self.frames
                        .last_mut()
                        .unwrap()
                        .exception_handlers
                        .push(ExceptionHandler::new(pattern.clone(), closure))
                }
                Instruction::Raise => {
                    let raised_value = self.stack.pop().unwrap();
                    self.raise(raised_value);
                }
                Instruction::BinOp(op) => {
                    let right = self.stack.pop().unwrap();
                    let left = self.stack.pop().unwrap();

                    let binop_result = match op {
                        Op::Add => left.add(right),
                        Op::Sub => left.sub(right),
                        Op::Mul => left.mul(right),
                        Op::Div => left.div(right),
                        Op::Eq => left.val_eq(&right),
                        Op::GtEq => {
                            match left.val_eq(&right) {
                                Ok(Value::Boolean(false)) => left.val_gt(&right),
                                yes => yes,
                            }
                        }
                        Op::Gt => left.val_gt(&right),
                        Op::LtEq => {
                            match left.val_eq(&right) {
                                Ok(Value::Boolean(false)) => left.val_lt(&right),
                                yes => yes,
                            }
                        }
                        Op::Lt => left.val_lt(&right),
                    };

                    if let Ok(result) = binop_result {
                        self.stack.push(result);
                    } else {
                        // TODO: Raise
                    }
                }
                Instruction::IndexAccess => {
                    let property = self.stack.pop().unwrap();
                    let target = self.stack.pop().unwrap();

                    match target {
                        Value::Map(ref map) => {
                            if let Some(value) = map.borrow().get(&property) {
                                self.stack.push((*value).clone());
                            } else {
                                panic!("no value for {:?}", target); // TODO: Raise
                            }
                        }
                        v => panic!("can't use index access for {:?}", v), // TODO: Raise
                    };
                }
                Instruction::IndexAssign => {
                    let value = self.stack.pop().unwrap();
                    let property = self.stack.pop().unwrap();
                    let mut target = self.stack.pop().unwrap();

                    match target {
                        Value::Map(ref mut map) => {
                            map.borrow_mut().insert(property, value);
                        }
                        v => panic!("can't use index access for {:?}", v), // TODO: Raise
                    };
                }
                Instruction::Import => {
                    let name = self.stack.pop().unwrap();
                    if let Value::CharString(ref str) = name {
                        if let Some(lib) = find_lib(str) {
                            self.stack.push(lib.clone());
                        }
                    } else {
                        panic!("import value must be a string"); // TODO: Raise
                    }
                }
                Instruction::Native(native_fn) => {
                    trace!("Starting native code");
                    let instructions = native_fn.call(self);
                    trace!("Finished native code");
                    self.reset_instructions(Rc::new(instructions), None)
                }
            };
        }
    }

    pub fn push(&mut self, value: Value) {
        self.stack.push(value);
    }

    pub fn pop(&mut self) -> Option<Value> {
        self.stack.pop()
    }

    pub fn fetch(&mut self, name: &String) -> Option<Value> {
        self.bindings().fetch(name)
    }

    pub fn local_assign(&mut self, name: &String, value: Value) {
        self.bindings().local_assign(name, value);
    }

    fn bindings(&mut self) -> &mut BindingMap {
        &mut self.frames.last_mut().unwrap().bindings
    }

    fn raise(&mut self, value: Value) {
        let matched_handler = self
            .frames
            .iter()
            .rev()
            .filter_map(|frame| {
                let handlers = frame
                    .exception_handlers
                    .iter()
                    .filter_map(|handler| match handler.matches(value.clone()) {
                        Some(bindings) => Some((handler, bindings)),
                        None => None,
                    })
                    .collect::<Vec<_>>();

                if handlers.is_empty() {
                    return None;
                }

                trace!("found handlers: {:?}", handlers.len());
                Some(handlers.first().unwrap().clone())
            })
            .take(1)
            .collect::<Vec<_>>()
            .first()
            .map(|&(ref handler, ref bindings)| {
                let mut map = BindingMap::new(Some(&handler.closure.parent_bindings));
                for (key, value) in bindings.iter() {
                    map.local_assign(key, value.to_owned());
                }
                trace!("bindings: {:?}", bindings);
                (handler.closure.instructions.clone(), map)
            });

        if let Some((instructions, map)) = matched_handler {
            trace!("instructions: {:?}", instructions);
            self.reset_instructions(instructions, Some(map));
        } else {
            debug!("Uncaught exception ignored: {:?}", value);
        }
    }

    fn reset_instructions(
        &mut self,
        instructions: Rc<InstructionSequence>,
        binding_map: Option<BindingMap>,
    ) {
        if let Some(map) = binding_map {
            self.frames.push(Frame::new(map));
        }
        self.instructions = instructions.clone();
        self.pc = 0;
        trace!("instructions have been reset!");
    }

    fn next_instruction(vm: &mut Vm) -> Option<Instruction> {
        let instruction = match vm.instructions.get(vm.pc) {
            Some(i) => Some(i.clone()),
            None => None,
        };

        vm.pc += 1;

        instruction
    }

    fn literal_to_value<'b>(literal: &'b Literal, top_bindings: &BindingMap) -> Value {
        match literal {
            &Literal::Number(ref num) => Value::Number(num.to_owned()),
            &Literal::CharString(ref str) => Value::CharString(str.to_string()),
            &Literal::Fn(ref args, ref statements) => {
                // Statements should be compiled ahead of time
                let closure = Closure::new(Rc::new(compile(&statements)), top_bindings);
                Value::Closure(Rc::new(args.clone()), Rc::new(closure))
            }
            &Literal::Boolean(b) => Value::Boolean(b),
            _ => panic!("not implemented literal_to_value for {:?}", literal),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_helpers::*;

    #[test]
    fn run_simple() {
        let mut vm = Vm::new(
            r#"let a = 1
        let b = { "a" => 1 }"#,
        );
        vm.run();
        assert_eq!(
            v_number(1, 1),
            vm.fetch(&"a".to_owned()).unwrap().to_owned()
        );
        let expected_map = v_map(vec![(v_string("a"), v_number(1, 1))]);
        assert_eq!(expected_map, vm.fetch(&"b".to_owned()).unwrap().to_owned())
    }

    #[test]
    fn function_call() {
        let source = r#"let a = ""
            let x = fn() do
              a = 1
            end
            x()"#;

        let mut vm = Vm::new(source);
        vm.run();
        assert_eq!(
            v_number(1, 1),
            vm.fetch(&"a".to_owned()).unwrap().to_owned()
        )
    }

    #[test]
    fn function_call_with_args() {
        let source = r#"let a = ""
            let b = ""
            let x = fn(c, d) do
              a = c
              b = d
            end
            x(1, 2)"#;

        let mut vm = Vm::new(source);
        vm.run();
        assert_eq!(
            v_number(1, 1),
            vm.fetch(&"a".to_owned()).unwrap().to_owned()
        );
        assert_eq!(
            v_number(2, 1),
            vm.frames
                .last()
                .unwrap()
                .bindings
                .fetch(&"b".to_owned())
                .unwrap()
                .to_owned()
        )
    }

    #[test]
    #[should_panic(expected = "expected a closure")]
    fn calling_non_function() {
        let source = r#"let x = ""
            x()"#;
        let mut vm = Vm::new(source);
        vm.run();
    }

    #[test]
    #[should_panic(expected = "wrong number of arguments")]
    fn function_with_wrong_arg_count() {
        let source = "let x = fn(a, b) do
            end
            x(1)";

        let mut vm = Vm::new(source);
        vm.run();
    }

    #[test]
    fn basic_rescue() {
        let source = r#"let a = ""
            rescue(id) do
                a = id
            end
            let x = fn(b) do
                raise(b)
            end
            x(1)"#;

        let mut vm = Vm::new(source);
        vm.run();
        assert_eq!(
            v_number(1, 1),
            vm.fetch(&"a".to_owned()).unwrap().to_owned()
        )
    }

    #[test]
    fn rescue_map() {
        let source = r#"let a = ""
            rescue({"b" => id}) do
                a = id
            end
            let x = fn(a, b) do
                raise({"a" => 1, "b" => b})
            end
            x(2, 1)"#;

        let mut vm = Vm::new(source);
        vm.run();
        assert_eq!(
            v_number(1, 1),
            vm.fetch(&"a".to_owned()).unwrap().to_owned()
        )
    }

    #[test]
    fn maps() {
        let source = r#"let a = { "c" => 1 }
            a["b"] = 2
            let b = a["b"]
            let c = a["c"]
            let d = a + { "e" => 3 }"#;

        let mut vm = Vm::new(source);
        vm.run();

        assert_eq!(
            v_number(2, 1),
            vm.fetch(&"b".to_owned()).unwrap().to_owned()
        );
        assert_eq!(
            v_number(1, 1),
            vm.fetch(&"c".to_owned()).unwrap().to_owned()
        );
        assert_eq!(
            v_map(vec![
                (v_string("c"), v_number(1, 1)),
                (v_string("b"), v_number(2, 1)),
            ]),
            vm.fetch(&"a".to_owned()).unwrap().to_owned()
        );
        assert_eq!(
            v_map(vec![
                (v_string("c"), v_number(1, 1)),
                (v_string("b"), v_number(2, 1)),
                (v_string("e"), v_number(3, 1)),
            ]),
            vm.fetch(&"d".to_owned()).unwrap().to_owned()
        );
    }

    #[test]
    fn fibonacci() {
        let source = r#"let fib = fn(k) do
          rescue({ "m" => m, "k" => 0 }) do
            raise({ "result" => m })
          end
          rescue({ "m" => m, "n" => n, "k" => k }) do
            raise({ "m" => n, "n" => m + n, "k" => k - 1 })
          end
          raise({ "m" => 0, "n" => 1, "k" => k })
        end
        let res = ""
        let setup = fn() do
          rescue({ "result" => r }) do
            res = r
          end
          fib(6)
        end
        setup()"#;

        let mut vm = Vm::new(source);
        vm.run();
        assert_eq!(
            v_number(8, 1),
            vm.fetch(&"res".to_owned()).unwrap().to_owned()
        )
    }

    #[test]
    fn import_file() {
        let mut buffer = File::create("read_test.txt").unwrap();

        buffer.write(b"file content").unwrap();

        let source = r#"let a = import("file")
             let res = ""
             rescue({"file.result" => r}) do
               res = r
             end
             a.read("read_test.txt")"#;

        let mut vm = Vm::new(source);
        vm.run();
        assert_eq!(
            v_string("file content"),
            vm.fetch(&"res".to_owned()).unwrap().to_owned()
        );

        fs::remove_file("read_test.txt").unwrap();
    }
}
