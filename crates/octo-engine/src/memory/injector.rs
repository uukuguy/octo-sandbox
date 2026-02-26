use octo_types::MemoryBlock;

pub struct ContextInjector;

impl ContextInjector {
    pub fn compile(blocks: &[MemoryBlock]) -> String {
        let mut output = String::from("<working_memory>\n");

        for block in blocks {
            let tag = &block.id;
            output.push_str(&format!("<{tag}>{}</{tag}>\n", block.value));
        }

        output.push_str("</working_memory>");
        output
    }
}
