pub mod gennasm;

use crate::checking;

trait Generator {
    const TARGET_NAME: &'static str;

    fn execute(mut self, instructions: Vec<checking::Instruction>) -> String where Self: Sized {
        for instruction in instructions {
            log::trace!("Code generator targing {} handling instruction: {:?}", Self::TARGET_NAME, instruction);

            self.handle_instruction(instruction);
        }

        self.construct_output()
    }

    fn handle_instruction(&mut self, instruction: checking::Instruction);

    fn construct_output(self) -> String;
}