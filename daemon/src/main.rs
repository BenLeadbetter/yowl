mod logging;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    crate::logging::init()?;
    println!("Hello, world!");
    Ok(())
}
