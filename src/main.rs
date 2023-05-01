use std::borrow::Borrow;
use winapi::{
    shared::ntdef::{HANDLE, NULL},
    shared::minwindef::{FALSE, LPVOID, DWORD, LPCVOID},
    shared::basetsd::DWORD64,
    um::winnt::PROCESS_ALL_ACCESS,
    um::processthreadsapi::OpenProcess,
    um::handleapi::CloseHandle,
    um::winuser::{FindWindowA, GetWindowThreadProcessId},
    um::errhandlingapi::GetLastError,
    um::handleapi::INVALID_HANDLE_VALUE,
    vc::vadefs::uintptr_t,
};
use std::time::Duration;

use speedy2d::color::Color;
use speedy2d::dimen::{UVec2};
use speedy2d::dimen::Vec2;

use speedy2d::window::{
    UserEventSender,
    WindowCreationOptions,
    WindowHandler,
    WindowHelper,
    WindowSize,
    WindowStartupInfo,
};
use speedy2d::{Graphics2D, Window};

use winapi::ctypes::__uint64;

struct Vector3f {
    x: f32,
    y: f32,
    z: f32,
}

impl Default for Vector3f {
    fn default() -> Self {
        Vector3f { x: 0., y: 0., z: 0. }
    }
}

struct Matrix {
    matrix: [f32; 16],
}

impl Default for Matrix {
    fn default() -> Self {
        Matrix {
            matrix: [0.0; 16],
        }
    }
}

const OFFSET_ENTITYLIST: u64 = 0x1f33f58;
const OFFSET_MATRIX: u64 = 0x1a93d0;
const OFFSET_RENDER: u64 = 0xd4138f0;
const OFFSET_ORIGIN: u64 = 0x014c;
const OFFSET_NAME: u64 = 0x0521;

fn Read<T: Default>(proc_h: HANDLE, address: DWORD64) -> T {
    use winapi::um::memoryapi::ReadProcessMemory;   // You need to enable this as feature in Cargo.toml
    let mut ret: T = Default::default();
    unsafe {
        let rpm_return = ReadProcessMemory(proc_h, address as *mut _,
                                           &mut ret as *mut T as LPVOID, std::mem::size_of::<T>(),
                                           NULL as *mut usize);
        if rpm_return == FALSE {
            //println!("ReadProcessMemory failed. Error: {:?}", std::io::Error::last_os_error());
        }
    }
    return ret;
}

fn Write<T: Default>(proc_h: HANDLE, address: u64, mut value: T) {
    use winapi::um::memoryapi::WriteProcessMemory;   // You need to enable this as feature in Cargo.toml

    unsafe {
        let wpm_return = WriteProcessMemory(proc_h, address as *mut _,
                                            &mut value as *mut T as LPCVOID, std::mem::size_of::<T>(),
                                            NULL as *mut usize);
        if wpm_return == FALSE {
            println!("WriteProcessMemory failed. Error: {:?}", std::io::Error::last_os_error());
        }
    }
}

fn getpid() -> u32 {
    unsafe {
        let title = std::ffi::CString::new("Apex Legends").unwrap();
        let hWnd = FindWindowA(std::ptr::null_mut(), title.as_ptr());
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hWnd, &mut pid);
        return pid;
    }
}

fn get_process_base_address(pid: DWORD) -> uintptr_t {
    use winapi::um::tlhelp32::{CreateToolhelp32Snapshot, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32,
                               MODULEENTRY32, Module32First, Module32Next};
    use winapi::um::handleapi::CloseHandle;
    let mut modBaseAddr: uintptr_t = 0;
    unsafe {
        let hSnap: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);

        if hSnap != INVALID_HANDLE_VALUE {
            let mut modEntry: MODULEENTRY32 = std::mem::zeroed();
            modEntry.dwSize = std::mem::size_of_val(&modEntry) as u32;
            modEntry.dwSize = std::mem::size_of::<winapi::um::tlhelp32::MODULEENTRY32>() as u32;
            if Module32First(hSnap, &mut modEntry) > 0 {
                loop {
                    let mut s: Vec<u8> = Vec::new();
                    for char in modEntry.szModule {
                        if char == 0 {
                            break;
                        }
                        s.push(char as _);
                    }
                    let str = String::from_utf8(s).unwrap();
                    if str.eq("r5apex.exe") {
                        modBaseAddr = modEntry.modBaseAddr as uintptr_t;
                        break;
                    }
                    Module32Next(hSnap, &mut modEntry);
                }
            }
        }
        CloseHandle(hSnap);
    }
    return modBaseAddr;
}

fn GetEntityById(Ent: i32, Base: DWORD64, pid: u32) -> DWORD64 {
    let proc_h: HANDLE = unsafe { OpenProcess(PROCESS_ALL_ACCESS, FALSE, pid as u32) };
    let EntityList = Base + OFFSET_ENTITYLIST;
    let BaseEntity = Read::<DWORD64>(proc_h, EntityList);
    if BaseEntity == 0 {
        return 0;
    }
    let entity_address = EntityList + (Ent << 5) as u64;
    return Read::<DWORD64>(proc_h, entity_address);
}

fn _WorldToScreen<'a>(pos: Vector3f, matrix: &'a Matrix) -> Vector3f {
    let mut out = Vector3f { x: 0., y: 0., z: 0. };
    let mut _x = matrix.matrix[0] * pos.x + matrix.matrix[1] * pos.y + matrix.matrix[2] * pos.z + matrix.matrix[3];
    let mut _y = matrix.matrix[4] * pos.x + matrix.matrix[5] * pos.y + matrix.matrix[6] * pos.z + matrix.matrix[7];
    out.z = matrix.matrix[12] * pos.x + matrix.matrix[13] * pos.y + matrix.matrix[14] * pos.z + matrix.matrix[15];

    _x *= 1. / out.z;
    _y *= 1. / out.z;

    let width = 1920.; //Change this to your resolution.
    let height = 1080.;

    out.x = width * 0.5;
    out.y = height * 0.5;

    out.x += 0.5 * _x * width + 0.5;
    out.y -= 0.5 * _y * height + 0.5;

    return out;
}

fn main() {
    let window: Window<String> = Window::new_with_user_events(
        "Speedy2D: User Events Example",
        WindowCreationOptions::new_windowed(
            WindowSize::PhysicalPixels(UVec2::new(1920, 1080)),
            None,
        )
            .with_maximized(true)
            .with_transparent(true)
            .with_always_on_top(true)
            .with_decorations(false)
            .with_mouse_passthrough(true),
    )
        .unwrap();
    // Creates a UserEventSender, which can be used to post custom
    // events to this event loop from another thread.
    //
    // It's also possible to create an event sender using
    // `WindowHelper::create_user_event_sender()`.

    let pid = getpid();
    println!("Process pid: {}", pid);
    let base_addr = get_process_base_address(pid);

    println!("Base Addr: {:?}", base_addr);
    let proc_h: HANDLE = unsafe { OpenProcess(PROCESS_ALL_ACCESS, FALSE, pid as u32) };
    let viewRenderer = Read::<__uint64>(proc_h, (base_addr as u64 + OFFSET_RENDER) as DWORD64);
    let viewMatrix = Read::<__uint64>(proc_h, viewRenderer + OFFSET_MATRIX);
    window.run_loop(MyWindowHandler { pid, base_addr, proc_h , viewRenderer, viewMatrix })
}


struct MyWindowHandler
{
    pid: u32,
    proc_h: HANDLE,
    base_addr: uintptr_t,
    viewRenderer: __uint64,
    viewMatrix: __uint64,
}

impl WindowHandler<String> for MyWindowHandler
{
    fn on_start(&mut self, _helper: &mut WindowHelper<String>, _info: WindowStartupInfo)
    {}

    fn on_user_event(&mut self, _helper: &mut WindowHelper<String>, user_event: String)
    {}

    fn on_draw(&mut self, _helper: &mut WindowHelper<String>, graphics: &mut Graphics2D)
    {
        graphics.clear_screen(Color::TRANSPARENT);
        use std::os::raw::c_char;
        let pid = self.pid;
        let base_addr = self.base_addr;
        let proc_h = self.proc_h;
        let viewRenderer = self.viewRenderer;
        let viewMatrix = self.viewMatrix;
        let mut m = Read::<Matrix>(proc_h, viewMatrix);

        for i in 1..=800 {
            let Entity = GetEntityById(i, base_addr as DWORD64, pid);
            if Read::<i32>(proc_h, Entity) == 0 || Entity == 0 {
                continue;
            }
            let mut num: [c_char; 11] = [0; 11];
            let nameptr = Read::<uintptr_t>(proc_h, Entity + 0x0518);
            if nameptr == 0 {
                continue;
            }
            let mut name = String::new();
            for j in 0..10 {
                num[j] = Read::<c_char>(proc_h, (nameptr + j) as DWORD64);
                name.push(num[j] as u8 as char);
            }
            let npc = String::from("npc_dummie");
            if name.eq(&npc) {
                let entFeet = Read::<Vector3f>(proc_h, Entity + OFFSET_ORIGIN);
                let mut entHead = Vector3f {
                    x: entFeet.x,
                    y: entFeet.y,
                    z: entFeet.z + 65.,
                };
                let w2sEntFeet = _WorldToScreen(entFeet, &m);
                if w2sEntFeet.z <= 0. {
                    continue;
                }
                let w2sEntHead = _WorldToScreen(entHead, &m);
                if w2sEntHead.z <= 0. {
                    continue;
                }
                println!("{} {} {}", w2sEntFeet.x, w2sEntFeet.y, w2sEntFeet.z);

                let height = (w2sEntHead.y.abs() - w2sEntFeet.y.abs()).abs();
                let width = height / 2.;
                let mut x = w2sEntFeet.x.clone() - (width / 2.);
                let mut y = w2sEntFeet.y.clone();
                let mut h = height;
                let mut w = width;
                let B: [[Vec2; 2]; 4] = [
                    [Vec2::new(x, y), Vec2::new(x + w, y)],
                    [Vec2::new(x + w, y - h), Vec2::new(x + w, y)],
                    [Vec2::new(x, y), Vec2::new(x, y - h)],
                    [Vec2::new(x + w , y - h), Vec2::new(x, y - h)]
                ];
                for i in 0..4 {
                    graphics.draw_line(B[i][0], B[i][1], 2.5, Color::BLUE);
                }
            }
        }
        _helper.request_redraw();
    }
}