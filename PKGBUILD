# Maintainer: Johann Woelper <woelper@gmail.com>
pkgname=oculante
pkgver=0.6.57
pkgrel=1
makedepends=('rust' 'cargo')
arch=('i686' 'x86_64' 'armv6h' 'armv7h')
pkgdesc="A minimalistic image viewer with analysis and editing tools"
license=('MIT')

build() {
    return 0
}

package() {
    cd $srcdir
    cargo install --root="$pkgdir" --git=https://github.com/woelper/oculante/
}
