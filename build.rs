// Embeds the icon, the UAC manifest (asInvoker: never asks for admin) and the
// version information into the .exe via app.rc.
// With the GNU toolchain it requires windres on the PATH (ships with MinGW-w64).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_resource::compile("app.rc", embed_resource::NONE)
            .manifest_required()
            .expect("failed to embed the Windows resources (is windres on the PATH?)");
    }
}
