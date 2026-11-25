//! D3D12.X on host D3D12.
//!
//! D3D12.X records commands from code inlined into the title, which writes
//! packets into the command list while its put pointer is below a limit and
//! calls the list's method otherwise. Our lists keep the limits at zero, so
//! every command arrives here as a call, to be run on host D3D12. The
//! title's GPU addresses are CPU addresses (console memory is unified).
//!
//! [`device`] is the device object titles create, [`dxgi`] its adapter side,
//! [`resource`] heaps and resources, [`fence`] fences, [`queue`] command
//! queues, [`list`] command allocators and lists, [`descriptor`]
//! descriptor heaps, [`root`] root signatures, [`query`] query heaps,
//! [`signature`] command signatures, [`pipeline`] pipeline states,
//! [`frame`] frame pacing, [`object`] what all console graphics objects
//! share, [`console`] which console model the title sees. Planned:
//! `present` (swap chain).

#[cfg(all(windows, feature = "diagnostics"))]
mod capture;
pub mod console;
#[cfg(all(windows, feature = "diagnostics"))]
mod copystat;
pub mod descriptor;
pub mod device;
#[cfg(windows)]
mod draw;
pub mod dxgi;
#[cfg(windows)]
mod dxc;
pub mod fence;
pub mod frame;
#[cfg(windows)]
mod host;
#[cfg(windows)]
mod indirect;
#[cfg(windows)]
mod inplace;
pub mod list;
pub mod object;
pub mod pipeline;
#[cfg(windows)]
mod primitive;
pub mod query;
pub mod queue;
pub mod resource;
pub mod root;
#[cfg(windows)]
mod sampler;
#[cfg(windows)]
pub mod shader_cache;
#[cfg(windows)]
mod memory;
#[cfg(windows)]
mod shadow;
pub mod signature;
#[cfg(windows)]
mod texture;
#[cfg(windows)]
mod watch;

#[cfg(windows)]
#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub mesh_shaders: bool,
    pub shader_model_6_6: bool,
    pub name: String,
    pub vram_bytes: usize,
}

#[cfg(windows)]
pub fn gpu_info() -> Option<GpuInfo> {
    let h = host::host()?;
    // Report the adapter the runtime device actually uses, not the one with
    // the most VRAM: on multi-GPU machines those can differ.
    let (name, vram_bytes) = adapter_info(&h.device).unwrap_or_else(|| ("unknown".to_owned(), 0));
    Some(GpuInfo {
        mesh_shaders: h.mesh_shaders(),
        shader_model_6_6: h.shader_model_6_6(),
        name,
        vram_bytes,
    })
}

#[cfg(windows)]
fn adapter_info(device: &windows::Win32::Graphics::Direct3D12::ID3D12Device) -> Option<(String, usize)> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, IDXGIAdapter3, IDXGIFactory4};
    // SAFETY: the device exposes its adapter LUID.
    let luid = unsafe { device.GetAdapterLuid() };
    // SAFETY: plain DXGI factory creation.
    let factory = unsafe { CreateDXGIFactory2::<IDXGIFactory4>(DXGI_CREATE_FACTORY_FLAGS(0)).ok()? };
    // SAFETY: looking up the adapter by LUID.
    let adapter: IDXGIAdapter3 = unsafe { factory.EnumAdapterByLuid(luid).ok()? };
    // SAFETY: reading the adapter's description.
    let desc = unsafe { adapter.GetDesc1().ok()? };
    let name = String::from_utf16_lossy(&desc.Description)
        .trim_end_matches('\0')
        .to_owned();
    Some((name, desc.DedicatedVideoMemory))
}

#[cfg(windows)]
pub use host::on_failure;
#[cfg(windows)]
pub use inplace::{title_memory_freeing, title_memory_section, title_memory_view};
#[cfg(windows)]
pub use watch::{
    title_memory_reading, title_memory_remapped, title_memory_remapping, title_memory_written,
    writing as kernel_will_write,
};

/// Total installed physical RAM in bytes, queried once from the OS.
#[cfg(windows)]
pub(crate) fn total_system_ram_bytes() -> Option<u64> {
    static TOTAL: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *TOTAL.get_or_init(|| {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        // SAFETY: plain data the call fills in.
        let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        // SAFETY: a sized structure.
        let ok = unsafe { GlobalMemoryStatusEx(&mut m) } != 0;
        (ok && m.ullTotalPhys > 0).then_some(m.ullTotalPhys)
    })
}
