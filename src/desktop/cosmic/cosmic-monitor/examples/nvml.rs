use nvml_wrapper::Nvml;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    //TODO: this powers on NVIDIA GPU
    let nvml = Nvml::init()?;
    let count = nvml.device_count()?;
    println!("Devices: {}", count);
    for i in 0..count {
        let device = nvml.device_by_index(i)?;
        let name = device.name()?;
        println!("{}: {}: {:?}", i, name, device.utilization_rates()?);
        if let Ok(list) = device.process_utilization_stats(None) {
            for stats in list {
                println!("  {:?}", stats);
            }
        }
        for process in device.running_graphics_processes()? {
            println!("  G: {:?}", process);
        }
        for process in device.running_compute_processes()? {
            println!("  C: {:?}", process);
        }
    }
    Ok(())
}
