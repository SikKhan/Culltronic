//! Windows smoke test: reject DX12 during startup, then render using Vulkan.
//! Run with `cargo run -p culltronic-ui --example verify_graphics_fallback`.

#[cfg(windows)]
#[path = "../src/graphics.rs"]
mod graphics;

#[cfg(windows)]
fn main() -> eframe::Result {
    use eframe::{egui, egui_wgpu::WgpuSetup, wgpu};

    struct SmokeApp;
    impl eframe::App for SmokeApp {
        fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label("Graphics fallback startup passed");
            });
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    // Leave the user's explicit override untouched and require the default
    // policy for this test, which intentionally needs more than one attempt.
    assert!(
        wgpu::Backends::from_env().is_none(),
        "unset WGPU_BACKEND for this test"
    );
    let mut attempts = 0;
    let mut started = false;
    let result = graphics::run_with_fallback(|mut configuration| {
        attempts += 1;
        if attempts == 1 {
            let WgpuSetup::CreateNew(setup) = &mut configuration.wgpu_setup else {
                panic!("expected new graphics setup");
            };
            assert_eq!(setup.instance_descriptor.backends, wgpu::Backends::DX12);
            setup.native_adapter_selector = Some(std::sync::Arc::new(|_, _| {
                Err("DX12 intentionally rejected by fallback smoke test".into())
            }));
        }
        let result = eframe::run_native(
            "Culltronic graphics fallback test",
            eframe::NativeOptions {
                wgpu_options: configuration,
                persist_window: false,
                run_and_return: true,
                ..Default::default()
            },
            Box::new(|cc| {
                assert_eq!(attempts, 2, "expected fallback to Vulkan");
                let render_state = cc
                    .wgpu_render_state
                    .as_ref()
                    .unwrap_or_else(|| panic!("missing render state"));
                assert_eq!(
                    render_state.adapter.get_info().backend,
                    wgpu::Backend::Vulkan
                );
                started = true;
                Ok(Box::new(SmokeApp))
            }),
        );
        (result, started)
    });
    assert!(started, "fallback must create the app");
    assert_eq!(attempts, 2);
    result?;
    println!("PASS: DX12 startup rejected; Vulkan fallback rendered and exited normally");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    println!("This smoke test exercises the Windows startup fallback.");
}
