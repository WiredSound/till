use crate::checking;
use super::Generator;

pub fn input(instructions: Vec<checking::Instruction>) -> String {
    GenerateElf64::new().execute(instructions)
}

struct GenerateElf64 {
    text_section: Vec<Instruction>,
    bss_section: Vec<Instruction>,
    rodata_section: Vec<Instruction>,
    num_label_counter: usize
}

impl GenerateElf64 {
    fn new() -> Self {
        GenerateElf64 {
            text_section: vec![
                Instruction::Comment(format!("Target: {}", Self::TARGET_NAME)),
                Instruction::Section("text".to_string()),
                Instruction::Extern("printf".to_string()),
                Instruction::Global("main".to_string()),
                Instruction::Label("main".to_string())
            ],
            bss_section: vec![Instruction::Section("bss".to_string())],
            rodata_section: vec![Instruction::Section("rodata".to_string())],
            num_label_counter: 0
        }
    }
}

const RETURN_INSTRUCTIONS: &'static [Instruction] = &[
    Instruction::Pop(Oprand::Register(Reg::BasePointer)), // Restore the base pointer of the previous frame.
    Instruction::Ret(16) // Shift stack pointer by 2 (remove old base pointer, return address) when returning.
];

const POP_AND_CMP_WITH_ZERO_INSTRUCTIONS: &'static [Instruction] = &[
    Instruction::Pop(Oprand::Register(Reg::Rax)),
    Instruction::Cmp { dest: Oprand::Register(Reg::Rax), src: Oprand::Value(Val::Int(0)) }
];

impl Generator for GenerateElf64 {
    const TARGET_NAME: &'static str = "Linux elf64";

    fn handle_instruction(&mut self, instruction: checking::Instruction) {
        match instruction {
            checking::Instruction::Allocate(id) => {
                self.bss_section.extend(vec![
                    Instruction::Label(var_label(id)),
                    Instruction::Reserve
                ]);
            }

            checking::Instruction::Push(val) => {
                let oprand = match val {
                    checking::Value::Num(num_val) => {
                        let label = literal_label(self.num_label_counter);
                        self.num_label_counter += 1;

                        self.rodata_section.extend(vec![
                            Instruction::Label(label.clone()),
                            Instruction::Declare(Val::Float(num_val))
                        ]);

                        Oprand::Address(Box::new(Oprand::Label(label)))
                    }

                    checking::Value::Variable(var_id) =>
                        Oprand::Address(Box::new(Oprand::Label(var_label(var_id)))),

                    checking::Value::Char(chr_val) =>
                        Oprand::Value(Val::Int(chr_val as isize)),

                    checking::Value::Bool(bool_val) =>
                        Oprand::Value(Val::Int(if bool_val { 1 } else { 0 }))
                };

                self.text_section.push(Instruction::Push(oprand));
            }

            checking::Instruction::Store(id) => {
                let label = var_label(id);

                // Store value on top of stack in .data section:
                self.text_section.push(Instruction::Pop(
                    Oprand::Address(Box::new(Oprand::Label(label)))
                ));
            }

            checking::Instruction::Parameter { store_in, param_number } => {
                // Store function argument in parameter variable:
                self.text_section.extend(vec![
                    Instruction::Mov {
                        dest: Oprand::Register(Reg::Rax),
                        src: Oprand::AddressDisplaced(Box::new(Oprand::Register(Reg::StackPointer)), 16 + (param_number * 8))
                    },
                    Instruction::Mov {
                        dest: Oprand::Address(Box::new(Oprand::Label(var_label(store_in)))),
                        src: Oprand::Register(Reg::Rax)
                    }
                ]);
            }

            checking::Instruction::Label(id) => { self.text_section.push(Instruction::Label(label(id))); }

            checking::Instruction::Function(id) => { 
                self.text_section.extend(vec![
                    Instruction::Label(func_label(id)),
                    // Preserve the base pointer of the previous frame:
                    Instruction::Push(Oprand::Register(Reg::BasePointer)),
                    // Create a new frame beginning at the current stack top:
                    Instruction::Mov {
                        dest: Oprand::Register(Reg::BasePointer),
                        src: Oprand::Register(Reg::StackPointer)
                    }
                ]);
            }

            checking::Instruction::CallExpectingVoid(id) => { self.text_section.push(Instruction::Call(func_label(id))); }

            checking::Instruction::CallExpectingValue(id) => {
                self.text_section.extend(vec![
                    Instruction::Call(func_label(id)),
                    // Place the function return value on the stack:
                    Instruction::Push(Oprand::Register(Reg::Rax))
                ]);
            }

            checking::Instruction::ReturnVoid => { self.text_section.extend_from_slice(RETURN_INSTRUCTIONS); }

            checking::Instruction::ReturnValue => {
                // Place function return value in register:
                self.text_section.push(Instruction::Pop(Oprand::Register(Reg::Rax)));
                self.text_section.extend_from_slice(RETURN_INSTRUCTIONS);
            }

            checking::Instruction::Display { value_type, line_number } => {
                // TODO: Support Num and Bool as well as Char...

                self.text_section.extend(vec![
                    // Load format string (first argument):
                    Instruction::Mov { dest: Oprand::Register(Reg::DestIndex), src: Oprand::Label("display_char".to_string()) },
                    // Load line number (second argument):
                    Instruction::Mov { dest: Oprand::Register(Reg::SrcIndex), src: Oprand::Value(Val::Int(line_number as isize)) },
                    // Load value to be displayed (third argument):
                    Instruction::Pop(Oprand::Register(Reg::Rdx)),
                    // Indicate 0 floating-point arguments:
                    Instruction::Mov { dest: Oprand::Register(Reg::Ax), src: Oprand::Value(Val::Int(0)) },
                    // Call printf function:
                    Instruction::Call("printf".to_string())
                ]);
            }

            checking::Instruction::Jump(id) => { self.text_section.push(Instruction::Jmp(label(id))); }

            checking::Instruction::JumpIfTrue(id) => {
                self.text_section.extend_from_slice(POP_AND_CMP_WITH_ZERO_INSTRUCTIONS);
                // Jump if top of stack not equal to 0:
                self.text_section.push(Instruction::Jne(label(id)));
            }

            checking::Instruction::JumpIfFalse(id) => {
                self.text_section.extend_from_slice(POP_AND_CMP_WITH_ZERO_INSTRUCTIONS);
                // Jump if top of stack equals 0:
                self.text_section.push(Instruction::Je(label(id)));
            }

            checking::Instruction::Equals => {
                self.text_section.extend(vec![
                    // Take first value in comparison off the stack:
                    Instruction::Pop(Oprand::Register(Reg::Rax)),
                    // Subtract that value by the second top value on stack:
                    Instruction::Sub {
                        dest: Oprand::Register(Reg::Rax),
                        src: Oprand::Address(Box::new(Oprand::Register(Reg::StackPointer)))
                    },
                    // Push the 16-bit flags register onto the stack:
                    Instruction::PushFlags,
                    // Pop the flags register into the lower two bytes of rax register:
                    Instruction::Pop(Oprand::Register(Reg::Ax)),
                    // Extract the value of the zero flag:
                    Instruction::Shr { dest: Oprand::Register(Reg::Ax), shift_by: 6 },
                    Instruction::BitwiseAnd { dest: Oprand::Register(Reg::Rax), src: Oprand::Value(Val::Int(1)) },
                    // Place the value of the zero flag onto the stack:
                    Instruction::Mov {
                        dest: Oprand::Address(Box::new(Oprand::Register(Reg::StackPointer))),
                        src: Oprand::Register(Reg::Rax)
                    }
                ]);
            }

            checking::Instruction::Add => self.add_arithmetic_instructions(Instruction::FpuAdd),
            checking::Instruction::Subtract => self.add_arithmetic_instructions(Instruction::FpuSubtract),
            checking::Instruction::Multiply => self.add_arithmetic_instructions(Instruction::FpuMultiply),
            checking::Instruction::Divide => self.add_arithmetic_instructions(Instruction::FpuDivide),

            checking::Instruction::GreaterThan => {
                self.add_comparison_instructions(vec![
                    // Extract the carry flag bit (indicates greater than when set in this instance):
                    Instruction::Shr { dest: Oprand::Register(Reg::Ax), shift_by: 8 }
                ]);
            }

            checking::Instruction::LessThan => {
                self.add_comparison_instructions(vec![
                    // Create second copy of FPU status word:
                    Instruction::Mov { dest: Oprand::Register(Reg::Bx), src: Oprand::Register(Reg::Ax) },
                    // Have carry flag as least significant bit of ax:
                    Instruction::Shr { dest: Oprand::Register(Reg::Ax), shift_by: 8 },
                    // Have zero flag as least significant bit of bx:
                    Instruction::Shr { dest: Oprand::Register(Reg::Bx), shift_by: 14 },
                    // Both carry flag and zero flag being 0 indicates less than:
                    Instruction::BitwiseOr { dest: Oprand::Register(Reg::Ax), src: Oprand::Register(Reg::Bx) },
                    Instruction::BitwiseNot(Oprand::Register(Reg::Ax))
                ]);
            }

            checking::Instruction::Not => {
                self.text_section.extend(vec![
                    // Perform bitwise not on value on top of stack:
                    Instruction::BitwiseNot(Oprand::Address(Box::new(Oprand::Register(Reg::StackPointer)))),
                    // Discard all bits except the least significant:
                    Instruction::BitwiseAnd {
                        dest: Oprand::Address(Box::new(Oprand::Register(Reg::StackPointer))),
                        src: Oprand::Value(Val::Int(1))
                    }
                ]);
            }
        }
    }

    fn construct_output(mut self) -> String {
        self.text_section.extend(vec![
            // OK status code:
            Instruction::Mov { dest: Oprand::Register(Reg::Rax), src: Oprand::Value(Val::Int(0)) },
            // Return from main:
            Instruction::Ret(0)
        ]);

        self.rodata_section.extend(vec![
            Instruction::Label("display_char".to_string()),
            Instruction::DeclareString(r"Line %u display (Char type): %c\n\0".to_string())
        ]);

        self.text_section.extend(self.bss_section.into_iter());
        self.text_section.extend(self.rodata_section.into_iter());

        self.text_section.into_iter().map(|x| x.intel_syntax()).collect::<Vec<String>>().join("")
    }
}

impl GenerateElf64 {
    fn two_stack_items_to_fpu_stack(&mut self) {
        self.text_section.extend(vec![
            Instruction::FpuReset,
            // Load second-to-top of stack onto FPU stack:
            Instruction::FpuPush(Oprand::AddressDisplaced(Box::new(Oprand::Register(Reg::StackPointer)), 8)),
            // Load top of stack onto FPU stack:
            Instruction::FpuPush(Oprand::Address(Box::new(Oprand::Register(Reg::StackPointer)))),
            // Move stack pointer:
            Instruction::Add { dest: Oprand::Register(Reg::StackPointer), src: Oprand::Value(Val::Int(8)) },
        ]);
    }

    fn add_arithmetic_instructions(&mut self, operation: Instruction) {
        self.two_stack_items_to_fpu_stack();

        self.text_section.extend(vec![
            // Perform the arithmetic operation:
            operation,
            // Move result from FPU stack to regular stack:
            Instruction::FpuPop(
                Oprand::Address(Box::new(Oprand::Register(Reg::StackPointer)))
            ),
        ]);
    }
    
    fn add_comparison_instructions(&mut self, operations: Vec<Instruction>) {
        self.two_stack_items_to_fpu_stack();
       
        self.text_section.extend(vec![
            // Compare items on FPU stack:
            Instruction::FpuCompare,
            // Store the FPU status register in ax:
            Instruction::FpuStatusReg(Oprand::Register(Reg::Ax)),
        ]);
        
        self.text_section.extend(operations);
        
        self.text_section.extend(vec![
            // Ensure all bits except the least significant one are clear:
            Instruction::BitwiseAnd { dest: Oprand::Register(Reg::Rax), src: Oprand::Value(Val::Int(1)) },
            //  Store result:
            Instruction::Mov {
                dest: Oprand::Address(Box::new(Oprand::Register(Reg::StackPointer))),
                src: Oprand::Register(Reg::Rax)
            }
        ]);
    }
}

/// Trait for conversion to Intel or AT&T assembly syntax.
trait AssemblyDisplay {
    fn intel_syntax(self) -> String;
    fn at_and_t_syntax(self) -> String where Self: Sized { unimplemented!() }
}

#[derive(Clone)]
enum Instruction {
    Comment(String),
    Section(String),
    Extern(String),
    Global(String),
    Label(String),
    Declare(Val),
    DeclareString(String),
    Syscall,
    Mov { dest: Oprand, src: Oprand },
    Add { dest: Oprand, src: Oprand },
    Sub { dest: Oprand, src: Oprand },
    Push(Oprand),
    Pop(Oprand),
    FpuPush(Oprand),
    FpuPop(Oprand),
    FpuStatusReg(Oprand),
    FpuReset,
    FpuCompare,
    FpuAdd,
    FpuSubtract,
    FpuMultiply,
    FpuDivide,
    Reserve,
    Ret(usize),
    Call(String),
    Jmp(String),
    Shr { dest: Oprand, shift_by: usize },
    BitwiseAnd { dest: Oprand, src: Oprand },
    BitwiseOr { dest: Oprand, src: Oprand },
    BitwiseNot(Oprand),
    PushFlags,
    Cmp { dest: Oprand, src: Oprand },
    Je(String),
    Jne(String)
}

impl AssemblyDisplay for Instruction {
    fn intel_syntax(self) -> String {
        match self {
            Instruction::Comment(x) => format!("; {}\n", x),
            Instruction::Section(x) => format!("section .{}\n", x),
            Instruction::Extern(x) => format!("extern {}\n", x),
            Instruction::Global(x) => format!("global {}\n", x),
            Instruction::Label(x) => format!("{}:\n", x),
            Instruction::Declare(x) => format!("dq {}\n", x.intel_syntax()),
            Instruction::DeclareString(x) => format!("db `{}`\n", x),
            Instruction::Syscall => format!("syscall\n"),
            Instruction::Mov { dest, src } => format!("mov {}, {}\n", dest.intel_syntax(), src.intel_syntax()),
            Instruction::Add { dest, src } => format!("add {}, {}\n", dest.intel_syntax(), src.intel_syntax()),
            Instruction::Sub { dest, src } => format!("sub {}, {}\n", dest.intel_syntax(), src.intel_syntax()),
            Instruction::Push(x) => format!("push qword {}\n", x.intel_syntax()),
            Instruction::Pop(x) => format!("pop qword {}\n", x.intel_syntax()),
            Instruction::FpuPush(x) => format!("fld qword {}\n", x.intel_syntax()),
            Instruction::FpuPop(x) => format!("fst qword {}\n", x.intel_syntax()),
            Instruction::FpuStatusReg(x) => format!("fstsw {}\n", x.intel_syntax()),
            Instruction::FpuReset => "finit\n".to_string(),
            Instruction::FpuCompare => "fcom\n".to_string(),
            Instruction::FpuAdd => "fadd\n".to_string(),
            Instruction::FpuSubtract => "fsub\n".to_string(),
            Instruction::FpuMultiply => "fmul\n".to_string(),
            Instruction::FpuDivide => "fdiv\n".to_string(),
            Instruction::Reserve => "resq 1\n".to_string(),
            Instruction::Ret(x) => format!("ret {}\n", x),
            Instruction::Call(x) => format!("call {}\n", x),
            Instruction::Jmp(x) => format!("jmp {}\n", x),
            Instruction::Shr { dest, shift_by } => format!("shr {}, {}\n", dest.intel_syntax(), shift_by),
            Instruction::BitwiseAnd { dest, src } => format!("and qword {}, {}\n", dest.intel_syntax(), src.intel_syntax()),
            Instruction::BitwiseOr { dest, src } => format!("or qword {}, {}\n", dest.intel_syntax(), src.intel_syntax()),
            Instruction::BitwiseNot(x) => format!("not qword {}\n", x.intel_syntax()),
            Instruction::PushFlags => "pushf\n".to_string(),
            Instruction::Cmp { dest, src } => format!("cmp {}, {}\n", dest.intel_syntax(), src.intel_syntax()),
            Instruction::Je(x) => format!("je {}\n", x),
            Instruction::Jne(x) => format!("jne {}\n", x)
        }
    }
}

#[derive(Clone)]
enum Oprand {
    Label(String),
    Value(Val),
    Register(Reg),
    Address(Box<Oprand>),
    AddressDisplaced(Box<Oprand>, usize),
}

impl AssemblyDisplay for Oprand {
    fn intel_syntax(self) -> String {
        match self {
            Oprand::Label(x) => x,
            Oprand::Value(x) => x.intel_syntax(),
            Oprand::Register(x) => x.intel_syntax(),
            Oprand::Address(x) => format!("[{}]", x.intel_syntax()),
            Oprand::AddressDisplaced(x, displacement) => format!("[{} + {}]", x.intel_syntax(), displacement)
        }
    }
}

#[derive(Clone)]
enum Val { Int(isize), Float(f64) }

impl AssemblyDisplay for Val {
    fn intel_syntax(self) -> String {
        match self {
            Val::Int(x) => x.to_string(),
            Val::Float(x) => format!("{:.16}", x)
        }
    }
}

#[derive(Clone)]
enum Reg { Rax, Ax, Bx, Rdx, StackPointer, BasePointer, DestIndex, SrcIndex }

impl AssemblyDisplay for Reg {
    fn intel_syntax(self) -> String {
        match self {
            Reg::Rax => "rax",
            Reg::Ax => "ax",
            Reg::Bx => "bx",
            Reg::Rdx => "rdx",
            Reg::StackPointer => "rsp",
            Reg::BasePointer => "rbp",
            Reg::DestIndex => "rdi",
            Reg::SrcIndex => "rsi"
        }.to_string()
    }
}

fn label(id: usize) -> String { format!("label{}", id) }

fn func_label(id: usize) -> String { format!("func{}", id) }

fn var_label(id: usize) -> String { format!("var{}", id) }

fn literal_label(counter: usize) -> String { format!("literal{}", counter) }

// TODO: Tests...