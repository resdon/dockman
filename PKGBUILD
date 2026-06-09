# Maintainer: resdon
pkgname=dockman
pkgver=0.1.0
pkgrel=1
pkgdesc="A Wayland dock application"
arch=('x86_64')
url="https://github.com/resdon/dockman"
license=('custom')
depends=('wayland' 'libxkbcommon' 'fontconfig' 'gcc-libs')
makedepends=('rust' 'cargo')
source=("${pkgname}::git+${url}.git")
sha256sums=('SKIP')

prepare() {
  cd "$pkgname"
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "$pkgname"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --frozen --release --all-features
}

check() {
  cd "$pkgname"
  cargo test --frozen --all-features
}

package() {
  cd "$pkgname"
  install -Dm755 "target/release/dockman" "$pkgdir/usr/bin/dockman"
  install -Dm644 "font.ttf" "$pkgdir/usr/share/dockman/font.ttf"
  install -Dm644 "menu.png" "$pkgdir/usr/share/dockman/menu.png"
}
