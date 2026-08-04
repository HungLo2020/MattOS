#!/bin/sh
set -eu

work=/tmp/mattos-native-toolchain-test
rm -rf "$work"
mkdir -p "$work"
cd "$work"

gcc --version
g++ --version
as --version
ld --version
make --version
gcc -v 2>gcc-v.txt
g++ -v 2>gxx-v.txt
gcc -print-search-dirs >gcc-search-dirs.txt
gcc -print-sysroot >gcc-sysroot.txt
gcc -dumpspecs >gcc-specs.txt
test "$(cat gcc-sysroot.txt)" = "/"
if grep -E '/home/|/tmp/|/usr/local/|/opt/' gcc-v.txt gxx-v.txt gcc-search-dirs.txt gcc-specs.txt; then
    echo "host path contamination in installed compiler behavior" >&2
    exit 1
fi

cat >hello.c <<'EOF'
#include <pthread.h>
#include <stdio.h>
static void *worker(void *unused) { (void)unused; return (void *)"hello from MattOS C"; }
int main(void) {
    pthread_t thread;
    void *result = 0;
    if (pthread_create(&thread, 0, worker, 0) || pthread_join(thread, &result)) return 1;
    puts((const char *)result);
    return 0;
}
EOF
gcc -O0 hello.c -pthread -o hello-o0
gcc -O2 -pie hello.c -pthread -o hello
test "$(./hello)" = "hello from MattOS C"
readelf -l hello | grep '/lib64/ld-linux-x86-64.so.2'
readelf -d hello
ldd ./hello
if command -v file >/dev/null 2>&1; then file ./hello; fi

cat >plugin.c <<'EOF'
int mattos_plugin(void) { return 42; }
EOF
cat >dltest.c <<'EOF'
#include <dlfcn.h>
#include <stdio.h>
int main(void) {
    void *handle = dlopen("./libplugin.so", RTLD_NOW);
    if (!handle) return 1;
    int (*value)(void) = (int (*)(void))dlsym(handle, "mattos_plugin");
    if (!value || value() != 42) return 2;
    puts("MattOS dlopen ok");
    return dlclose(handle) != 0;
}
EOF
gcc -O2 -fPIC -shared plugin.c -o libplugin.so
gcc -O2 dltest.c -ldl -o dltest
test "$(./dltest)" = "MattOS dlopen ok"

cat >hello.cc <<'EOF'
#include <exception>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
int main() {
    std::vector<std::string> words{"hello", "from", "MattOS", "C++"};
    try { throw std::runtime_error(words[2]); }
    catch (const std::exception &error) {
        std::cout << words[0] << ' ' << words[1] << ' ' << error.what() << ' ' << words[3] << '\n';
    }
}
EOF
g++ -O2 hello.cc -o hello-cpp
test "$(./hello-cpp)" = "hello from MattOS C++"
readelf -l hello-cpp | grep '/lib64/ld-linux-x86-64.so.2'
readelf -d hello-cpp
readelf --version-info hello-cpp | grep -E 'GLIBCXX_|CXXABI_|GCC_'
ldd ./hello-cpp

cat >answer.s <<'EOF'
.text
.globl archive_answer
.type archive_answer,@function
archive_answer:
    mov $42, %eax
    ret
EOF
cat >archive-main.c <<'EOF'
#include <stdio.h>
extern int archive_answer(void);
int main(void) { printf("archive:%d\n", archive_answer()); return archive_answer() != 42; }
EOF
as answer.s -o answer.o
gcc -c archive-main.c -o archive-main.o
ar rcs libanswer.a answer.o
ranlib libanswer.a
nm libanswer.a | grep archive_answer
objdump -d libanswer.a | grep archive_answer
readelf -h answer.o
gcc archive-main.o libanswer.a -o archive-test
test "$(./archive-test)" = "archive:42"
cp archive-test archive-test.stripped
strip archive-test.stripped

cat >Makefile <<'EOF'
CC = gcc
CFLAGS = -O2
all: make-hello
make-hello: hello.c
	$(CC) $(CFLAGS) $< -pthread -o $@
clean:
	rm -f make-hello
EOF
make
test "$(./make-hello)" = "hello from MattOS C"
make clean
make

package_root="$work/package-root"
mkdir -p "$package_root/DEBIAN" "$package_root/usr/bin"
cp hello "$package_root/usr/bin/mattos-native-hello"
cat >"$package_root/DEBIAN/control" <<'EOF'
Package: mattos-native-test
Version: 1.0-1
Architecture: amd64
Maintainer: MattOS Test <test@mattos.invalid>
Description: Ephemeral native compiler package test
EOF
dpkg-deb --build --root-owner-group "$package_root" mattos-native-test.deb
dpkg-deb --info mattos-native-test.deb
dpkg-deb --contents mattos-native-test.deb
sudo dpkg -i mattos-native-test.deb
dpkg-query -S /usr/bin/mattos-native-hello | grep '^mattos-native-test:'
test "$(mattos-native-hello)" = "hello from MattOS C"
sudo dpkg -r mattos-native-test

echo __MATTOS_NATIVE_TOOLCHAIN_OK__
