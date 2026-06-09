# Maintainer: resdon
pkgname=dockman
pkgver=0.1.0
pkgrel=1
pkgdesc="A Wayland dock application"
arch=('x86_64')
license=('custom')
depends=('wayland' 'libxkbcommon' 'fontconfig' 'gcc-libs')
makedepends=('rust' 'cargo')
source=('Cargo.toml' 'src' 'font.ttf' 'launcher.sh' 'menu.png')
sha256sums=('SKIP' 'SKIP' 'SKIP' 'SKIP' 'SKIP')

prepare() {
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --frozen --release --all-features
}

check() {
  cargo test --frozen --all-features
}

package() {
  install -Dm755 "target/release/dockman" "$pkgdir/usr/bin/dockman"
  install -Dm755 "launcher.sh" "$pkgdir/usr/share/dockman/launcher.sh"
  install -Dm644 "font.ttf" "$pkgdir/usr/share/dockman/font.ttf"
  install -Dm644 "menu.png" "$pkgdir/usr/share/dockman/menu.png"
}
