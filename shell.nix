{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  packages = with pkgs; [
    rustup
    rust-analyzer
  ];

  shellHook = ''
    # setup default toolchain kalau belum ada
    if ! rustup show active-toolchain >/dev/null 2>&1; then
      echo "Setting up default Rust toolchain (stable)..."
      rustup toolchain install stable
      rustup default stable
    fi

    echo "Rust dev shell siap. Gunakan 'rustup' untuk kelola toolchain."
  '';
}
