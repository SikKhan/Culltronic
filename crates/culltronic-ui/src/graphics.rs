//! Platform graphics defaults, startup fallback, and diagnostic overrides.

use eframe::egui_wgpu::WgpuConfiguration;

/// Run with Windows startup fallback; explicit overrides and other platforms
/// retain their backend selection. The runner reports whether app creation began
/// so a later rendering error cannot reopen an already-running session.
pub(crate) fn run_with_fallback(
    run: impl FnMut(WgpuConfiguration) -> (eframe::Result, bool),
) -> eframe::Result {
    #[cfg(windows)]
    let configurations = windows_configurations(eframe::wgpu::Backends::from_env());
    #[cfg(not(windows))]
    let configurations = vec![WgpuConfiguration::default()];
    run_attempts(configurations, run)
}

fn run_attempts(
    configurations: Vec<WgpuConfiguration>,
    mut run: impl FnMut(WgpuConfiguration) -> (eframe::Result, bool),
) -> eframe::Result {
    let mut configurations = configurations.into_iter().peekable();
    while let Some(configuration) = configurations.next() {
        let (result, app_started) = run(configuration);
        match result {
            Err(eframe::Error::Wgpu(error)) if !app_started && configurations.peek().is_some() => {
                tracing::warn!(%error, "graphics startup failed; trying the next backend");
            }
            result => return result,
        }
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn windows_configurations(
    backend_override: Option<eframe::wgpu::Backends>,
) -> Vec<WgpuConfiguration> {
    use eframe::wgpu::Backends;
    // https://github.com/gfx-rs/wgpu/issues/9911: prefer DX12 to avoid
    // Intel Arc's native Vulkan resize crash. Try alternatives only if
    // startup fails, never enable Vulkan alongside DX12 in the first attempt.
    let backends = backend_override.map_or_else(
        || vec![Backends::DX12, Backends::VULKAN, Backends::GL],
        |requested| vec![requested],
    );
    backends
        .into_iter()
        .map(|backends| {
            let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
            setup.instance_descriptor.backends = backends;
            WgpuConfiguration {
                wgpu_setup: setup.into(),
                ..Default::default()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::{egui_wgpu::WgpuSetup, wgpu::Backends};

    fn backends(configuration: WgpuConfiguration) -> Backends {
        let WgpuSetup::CreateNew(setup) = configuration.wgpu_setup else {
            panic!("startup must create a graphics device");
        };
        setup.instance_descriptor.backends
    }

    fn graphics_error() -> eframe::Error {
        eframe::Error::Wgpu(eframe::egui_wgpu::WgpuError::NoSurfaceFormatsAvailable)
    }

    #[test]
    fn windows_tries_dx12_first_and_honors_explicit_overrides() {
        assert_eq!(
            windows_configurations(None)
                .into_iter()
                .map(backends)
                .collect::<Vec<_>>(),
            [Backends::DX12, Backends::VULKAN, Backends::GL]
        );
        for requested in [Backends::VULKAN, Backends::GL | Backends::DX12] {
            assert_eq!(
                windows_configurations(Some(requested))
                    .into_iter()
                    .map(backends)
                    .collect::<Vec<_>>(),
                [requested]
            );
        }
    }

    #[test]
    fn graphics_startup_failure_falls_back_and_stops_after_success() {
        let mut attempted = Vec::new();
        let result = run_attempts(windows_configurations(None), |configuration| {
            attempted.push(backends(configuration));
            if attempted.len() == 1 {
                (Err(graphics_error()), false)
            } else {
                (Ok(()), true)
            }
        });
        assert!(result.is_ok());
        assert_eq!(attempted, [Backends::DX12, Backends::VULKAN]);
    }

    #[test]
    fn all_graphics_failures_return_the_last_error() {
        let mut attempts = 0;
        let result = run_attempts(windows_configurations(None), |_| {
            attempts += 1;
            (Err(graphics_error()), false)
        });
        assert!(matches!(result, Err(eframe::Error::Wgpu(_))));
        assert_eq!(attempts, 3);
    }

    #[test]
    fn normal_exit_runtime_failure_and_app_creation_failure_never_retry() {
        for (result, app_started) in [
            (Ok(()), true),
            (Err(graphics_error()), true),
            (
                Err(eframe::Error::AppCreation(
                    std::io::Error::other("app failed").into(),
                )),
                false,
            ),
        ] {
            let mut outcome = Some((result, app_started));
            let _ = run_attempts(windows_configurations(None), |_| {
                outcome.take().unwrap_or_else(|| panic!("must not retry"))
            });
            assert!(outcome.is_none());
        }
    }

    #[test]
    fn explicit_backend_failure_never_tries_another_backend() {
        let mut attempts = 0;
        let result = run_attempts(windows_configurations(Some(Backends::DX12)), |_| {
            attempts += 1;
            (Err(graphics_error()), false)
        });
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }
}
