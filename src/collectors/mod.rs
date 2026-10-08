pub mod nic;
pub mod process;

pub mod etw {
    //! ETW integration boundary.
    //!
    //! RC1 deliberately keeps the Kernel-Network consumer isolated here.
    //! RC2 will attach Microsoft-Windows-Kernel-Network through ferrisetw and
    //! publish PID-scoped traffic events to the central state layer.
}
