"""Installed, unprivileged regression probes for the native upstream cohort.

These execute the installed libraries, plugins and GnuPG tools in the guest.
They use disposable fixtures and never access the user's keyring.
"""
from __future__ import annotations

import base64
import shlex
import textwrap
import zlib
from uuid import uuid4


GLIB = r'''
import ctypes as c
import tempfile
from pathlib import Path
lib = c.CDLL('libglib-2.0.so.0')
lib.g_utf8_validate.argtypes = [c.c_char_p, c.c_ssize_t, c.c_void_p]
lib.g_utf8_validate.restype = c.c_int
assert lib.g_utf8_validate('MattOS café 日本語'.encode(), -1, None)
assert not lib.g_utf8_validate(b'\xff', 1, None)
gio = c.CDLL('libgio-2.0.so.0')
gio.g_file_new_for_path.argtypes = [c.c_char_p]
gio.g_file_new_for_path.restype = c.c_void_p
gio.g_file_load_contents.argtypes = [c.c_void_p, c.c_void_p, c.POINTER(c.c_void_p), c.POINTER(c.c_size_t), c.c_void_p, c.c_void_p]
gio.g_file_load_contents.restype = c.c_int
lib.g_free.argtypes = [c.c_void_p]
obj = c.CDLL('libgobject-2.0.so.0')
obj.g_object_unref.argtypes = [c.c_void_p]
with tempfile.TemporaryDirectory() as tmp:
    path = Path(tmp) / 'unicode.txt'
    data = 'MattOS café 日本語\n'.encode()
    path.write_bytes(data)
    file = gio.g_file_new_for_path(bytes(path))
    contents, length = c.c_void_p(), c.c_size_t()
    try:
        assert gio.g_file_load_contents(file, None, c.byref(contents), c.byref(length), None, None)
        assert c.string_at(contents, length.value) == data
    finally:
        lib.g_free(contents)
        obj.g_object_unref(file)
print('PASS GLib UTF-8 validation and GIO file loading')
'''

PIXBUF = r'''
import base64
import ctypes as c
import struct
import tempfile
import zlib
from pathlib import Path
lib = c.CDLL('libgdk_pixbuf-2.0.so.0')
lib.gdk_pixbuf_new_from_file.argtypes = [c.c_char_p, c.c_void_p]
lib.gdk_pixbuf_new_from_file.restype = c.c_void_p
for name in ('width', 'height'):
    fn = getattr(lib, 'gdk_pixbuf_get_' + name)
    fn.argtypes, fn.restype = [c.c_void_p], c.c_int
obj = c.CDLL('libgobject-2.0.so.0')
obj.g_object_unref.argtypes = [c.c_void_p]
def chunk(kind, data):
    return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 1, 1, 8, 2, 0, 0, 0))
png += chunk(b'IDAT', zlib.compress(b'\x00\xff\x00\x00')) + chunk(b'IEND', b'')
gif = base64.b64decode('R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7')
with tempfile.TemporaryDirectory() as tmp:
    for name, data in (('native.png', png), ('module.gif', gif)):
        path = Path(tmp) / name
        path.write_bytes(data)
        image = lib.gdk_pixbuf_new_from_file(bytes(path), None)
        assert image, name
        try:
            assert lib.gdk_pixbuf_get_width(image) == lib.gdk_pixbuf_get_height(image) == 1
        finally:
            obj.g_object_unref(image)
print('PASS GdkPixbuf built-in PNG and cached GIF module decoding')
'''

XKB = r'''
import ctypes as c
lib = c.CDLL('libxkbcommon.so.0')
class Names(c.Structure):
    _fields_ = [(name, c.c_char_p) for name in ('rules','model','layout','variant','options')]
lib.xkb_context_new.argtypes, lib.xkb_context_new.restype = [c.c_int], c.c_void_p
lib.xkb_keymap_new_from_names.argtypes = [c.c_void_p, c.POINTER(Names), c.c_int]
lib.xkb_keymap_new_from_names.restype = c.c_void_p
lib.xkb_keymap_key_by_name.argtypes, lib.xkb_keymap_key_by_name.restype = [c.c_void_p, c.c_char_p], c.c_uint
lib.xkb_state_new.argtypes, lib.xkb_state_new.restype = [c.c_void_p], c.c_void_p
lib.xkb_state_key_get_one_sym.argtypes, lib.xkb_state_key_get_one_sym.restype = [c.c_void_p, c.c_uint], c.c_uint
for name in ('xkb_context_unref', 'xkb_keymap_unref', 'xkb_state_unref'):
    getattr(lib, name).argtypes = [c.c_void_p]
ctx = lib.xkb_context_new(0)
assert ctx
try:
    for layout, expected in ((b'us', ord('y')), (b'de', ord('z'))):
        names = Names(b'evdev', b'pc105', layout, b'', b'')
        keymap = lib.xkb_keymap_new_from_names(ctx, c.byref(names), 0)
        assert keymap, layout
        state = lib.xkb_state_new(keymap)
        assert state
        try:
            key = lib.xkb_keymap_key_by_name(keymap, b'AD06')
            assert lib.xkb_state_key_get_one_sym(state, key) == expected
        finally:
            lib.xkb_state_unref(state)
            lib.xkb_keymap_unref(keymap)
finally:
    lib.xkb_context_unref(ctx)
print('PASS libxkbcommon US/German keyboard compilation and translation')
'''

ARCHIVE = r'''
import ctypes as c
import gzip
import io
import tarfile
import zipfile
lib = c.CDLL('libarchive.so.13')
lib.archive_read_new.restype = c.c_void_p
for name in ('archive_read_support_filter_all','archive_read_support_format_all','archive_read_free'):
    getattr(lib, name).argtypes = [c.c_void_p]
lib.archive_read_open_memory.argtypes = [c.c_void_p, c.c_void_p, c.c_size_t]
lib.archive_read_next_header.argtypes = [c.c_void_p, c.POINTER(c.c_void_p)]
lib.archive_read_data.argtypes, lib.archive_read_data.restype = [c.c_void_p, c.c_void_p, c.c_size_t], c.c_ssize_t
lib.archive_entry_pathname.argtypes, lib.archive_entry_pathname.restype = [c.c_void_p], c.c_char_p
payload = b'MattOS archive roundtrip\n'
tar = io.BytesIO()
with tarfile.open(fileobj=tar, mode='w') as writer:
    entry = tarfile.TarInfo('payload.txt')
    entry.size = len(payload)
    writer.addfile(entry, io.BytesIO(payload))
zip_data = io.BytesIO()
with zipfile.ZipFile(zip_data, 'w', compression=zipfile.ZIP_DEFLATED) as writer:
    writer.writestr('payload.txt', payload)
for data in (tar.getvalue(), gzip.compress(tar.getvalue()), zip_data.getvalue()):
    reader = lib.archive_read_new()
    buffer = c.create_string_buffer(data)
    try:
        assert lib.archive_read_support_filter_all(reader) == 0
        assert lib.archive_read_support_format_all(reader) == 0
        assert lib.archive_read_open_memory(reader, buffer, len(data)) == 0
        entry = c.c_void_p()
        assert lib.archive_read_next_header(reader, c.byref(entry)) == 0
        assert lib.archive_entry_pathname(entry) == b'payload.txt'
        output = c.create_string_buffer(256)
        length = lib.archive_read_data(reader, output, len(output))
        assert length == len(payload) and output.raw[:length] == payload
        assert lib.archive_read_next_header(reader, c.byref(entry)) == 1
    finally:
        lib.archive_read_free(reader)
# A recognized but truncated ZIP must fail, rather than read beyond its buffer.
data = zip_data.getvalue()[:35]
reader = lib.archive_read_new()
buffer = c.create_string_buffer(data)
try:
    lib.archive_read_support_filter_all(reader)
    lib.archive_read_support_format_all(reader)
    result = lib.archive_read_open_memory(reader, buffer, len(data))
    if result == 0:
        entry = c.c_void_p()
        result = lib.archive_read_next_header(reader, c.byref(entry))
        if result == 0:
            output = c.create_string_buffer(256)
            result = lib.archive_read_data(reader, output, len(output))
    assert result < 0, result
finally:
    lib.archive_read_free(reader)
print('PASS libarchive tar/gzip/ZIP decoding and truncated ZIP rejection')
'''

GSTREAMER = r'''
import ctypes as c
lib = c.CDLL('libgstreamer-1.0.so.0')
lib.gst_init.argtypes = [c.c_void_p, c.c_void_p]
lib.gst_init(None, None)
lib.gst_parse_launch.argtypes, lib.gst_parse_launch.restype = [c.c_char_p, c.POINTER(c.c_void_p)], c.c_void_p
lib.gst_element_set_state.argtypes = [c.c_void_p, c.c_int]
lib.gst_element_get_bus.argtypes, lib.gst_element_get_bus.restype = [c.c_void_p], c.c_void_p
lib.gst_bus_timed_pop_filtered.argtypes, lib.gst_bus_timed_pop_filtered.restype = [c.c_void_p, c.c_uint64, c.c_uint], c.c_void_p
lib.gst_message_unref.argtypes = [c.c_void_p]
lib.gst_object_unref.argtypes = [c.c_void_p]
error = c.c_void_p()
pipeline = lib.gst_parse_launch(b'audiotestsrc num-buffers=16 ! audioconvert ! audioresample ! fakesink', c.byref(error))
assert pipeline and not error.value, 'installed core/base plugin discovery failed'
bus = lib.gst_element_get_bus(pipeline)
try:
    assert lib.gst_element_set_state(pipeline, 4) != 0
    message = lib.gst_bus_timed_pop_filtered(bus, 10_000_000_000, 1)
    assert message, 'audio pipeline did not reach EOS'
    lib.gst_message_unref(message)
finally:
    lib.gst_element_set_state(pipeline, 1)
    lib.gst_object_unref(bus)
    lib.gst_object_unref(pipeline)
print('PASS GStreamer installed core/base audio conversion pipeline reached EOS')
'''

GNUPG = r'''
import os
import subprocess
import tempfile
from pathlib import Path
with tempfile.TemporaryDirectory(prefix='mattos-gpg-') as tmp:
    os.chmod(tmp, 0o700)
    env = dict(os.environ, GNUPGHOME=tmp)
    # Do not inherit the developer's agent socket or keyring configuration.
    env.pop('GPG_AGENT_INFO', None)
    def run(*args, check=True):
        result = subprocess.run(args, env=env, timeout=60, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if check and result.returncode:
            raise RuntimeError(f'{args}: {result.stderr.decode(errors="replace")}')
        return result
    try:
        run('gpg', '--batch', '--pinentry-mode', 'loopback', '--passphrase', '', '--quick-generate-key', 'MattOS disposable validation <test@invalid>', 'ed25519', 'sign', '0')
        data = Path(tmp) / 'message.txt'
        data.write_bytes(b'MattOS signature roundtrip\n')
        run('gpg', '--batch', '--pinentry-mode', 'loopback', '--passphrase', '', '--detach-sign', str(data))
        key = Path(tmp) / 'public.gpg'
        key.write_bytes(run('gpg', '--batch', '--export').stdout)
        signature = str(data) + '.sig'
        run('gpgv', '--keyring', str(key), signature, str(data))
        data.write_bytes(b'tampered\n')
        assert run('gpgv', '--keyring', str(key), signature, str(data), check=False).returncode != 0
    finally:
        run('gpgconf', '--kill', 'all', check=False)
print('PASS GnuPG key generation, signing, gpgv verification and tamper rejection')
'''


KDE_STACK = r'''
import math
import os
import shlex
import struct
import wave
import subprocess
import tempfile
from pathlib import Path
source = r"""
#include <QGuiApplication>
#include <QAbstractEventDispatcher>
#include <PulseAudioQt/Context>
#include <QAudioDecoder>
#include <QAudioBuffer>
#include <QUrl>
#include <QImage>
#include <QPainter>
#include <QFont>
#include <QEventLoop>
#include <QTimer>
#include <QtGlobal>
#include <qt6keychain/keychain.h>
#include <CssParser.h>
#include <Union/StyleRegistry.h>
#include <Union/Style.h>
#include <tesseract/capi.h>
#include <iostream>
#include <stdexcept>
#include <string>

void require(bool ok, const char *message) {
    if (!ok) throw std::runtime_error(message);
}
void runJob(QKeychain::Job &job, QKeychain::Error expected = QKeychain::NoError) {
    job.setAutoDelete(false);
    bool done = false;
    QEventLoop loop;
    QObject::connect(&job, &QKeychain::Job::finished, &loop, [&] { done = true; loop.quit(); });
    QTimer::singleShot(0, &job, [&] { job.start(); });
    QTimer::singleShot(15000, &loop, &QEventLoop::quit);
    loop.exec();
    require(done, "QtKeychain job timed out");
    if (job.error() != expected) throw std::runtime_error(job.errorString().toStdString());
}
int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    try {
        require(std::string(qVersion()) == QT_VERSION_STR, "Qt runtime differs from its installed SDK");
        require(QByteArray(QAbstractEventDispatcher::instance()->metaObject()->className()).contains("Glib"),
                "Qt has no GLib dispatcher; Plasma audio controls cannot connect");
        auto *pulse = PulseAudioQt::Context::instance();
        QEventLoop pulseLoop;
        QTimer pulsePoll;
        QObject::connect(&pulsePoll, &QTimer::timeout, &pulseLoop, [&] {
            if (pulse->state() == PulseAudioQt::Context::State::Ready && !pulse->sinks().isEmpty()) pulseLoop.quit();
        });
        pulsePoll.start(25);
        QTimer::singleShot(15000, &pulseLoop, &QEventLoop::quit);
        pulseLoop.exec();
        require(pulse->state() == PulseAudioQt::Context::State::Ready && !pulse->sinks().isEmpty(),
                "PulseAudioQt did not connect to PipeWire and enumerate an output sink");
        std::cout << "PASS Qt GLib dispatcher and PulseAudioQt audio sink enumeration" << std::endl;
        cssparser::StyleSheet sheet{"validation.css"};
        sheet.parseString(R"css(@property mattos-probe-color { syntax: "<color>"; inherits: false; initial-value: black; }
            button { mattos-probe-color: #123456; })css");
        for (const auto &error : sheet.errors()) std::cerr << error.message << std::endl;
        require(sheet.errors().empty() && sheet.rules().size() == 1, "CSS Rust/C++ bridge could not parse a rule");
        auto registry = Union::StyleRegistry::instance();
        registry->load();
        auto style = registry->style(QStringLiteral("breeze"));
        require(bool(style) && !style->hasErrors() && !style->rules().empty(), "Union could not load its installed Breeze CSS plugin/data");
        std::cout << "PASS Qt " << qVersion() << ", CSS parser and Union style loading" << std::endl;

        QImage image(1000, 180, QImage::Format_Grayscale8);
        image.fill(Qt::white);
        QPainter painter(&image);
        painter.setPen(Qt::black);
        painter.setFont(QFont(QStringLiteral("Open Sans"), 54));
        painter.drawText(40, 115, QStringLiteral("HELLO WORLD"));
        painter.end();
        TessBaseAPI *api = TessBaseAPICreate();
        require(api != nullptr, "Tesseract allocation failed");
        if (TessBaseAPIInit3(api, nullptr, "eng") != 0) {
            TessBaseAPIDelete(api);
            throw std::runtime_error("offline English OCR model did not initialize");
        }
        TessBaseAPISetPageSegMode(api, tesseract::PSM_SINGLE_LINE);
        TessBaseAPISetImage(api, image.constBits(), image.width(), image.height(), 1, image.bytesPerLine());
        TessBaseAPISetSourceResolution(api, 96);
        char *text = TessBaseAPIGetUTF8Text(api);
        std::string recognized = text ? text : "";
        TessDeleteText(text);
        TessBaseAPIDelete(api);
        std::cout << "OCR: " << recognized << std::endl;
        require(recognized.find("HELLO WORLD") != std::string::npos,
                "Qt-rendered screenshot text did not round-trip through OCR");
        std::cout << "PASS Qt font rendering, Leptonica/Tesseract and offline English OCR" << std::endl;

        QAudioDecoder decoder;
        QEventLoop audioLoop;
        bool audioFinished = false;
        int audioFrames = 0;
        QString audioError;
        QObject::connect(&decoder, &QAudioDecoder::bufferReady, &audioLoop, [&] {
            const QAudioBuffer buffer = decoder.read();
            if (!buffer.isValid() || buffer.format().sampleRate() != 16000 || buffer.format().channelCount() != 1) {
                audioError = QStringLiteral("unexpected decoded WAV format");
                audioLoop.quit();
                return;
            }
            audioFrames += buffer.frameCount();
        });
        QObject::connect(&decoder, &QAudioDecoder::finished, &audioLoop, [&] {
            audioFinished = true;
            audioLoop.quit();
        });
        QObject::connect(&decoder, qOverload<QAudioDecoder::Error>(&QAudioDecoder::error), &audioLoop, [&](QAudioDecoder::Error) {
            audioError = decoder.errorString();
            audioLoop.quit();
        });
        QTimer::singleShot(15000, &audioLoop, &QEventLoop::quit);
        decoder.setSource(QUrl::fromLocalFile(QString::fromUtf8(argv[2])));
        decoder.start();
        audioLoop.exec();
        if (!audioError.isEmpty()) throw std::runtime_error(audioError.toStdString());
        require(audioFinished && audioFrames == 4000, "Qt Multimedia did not decode the complete WAV fixture");
        std::cout << "PASS Qt Multimedia installed backend decoded 4000 audio frames" << std::endl;

        const QString service = QString::fromUtf8(argv[1]);
        QKeychain::WritePasswordJob write(service);
        write.setKey(QStringLiteral("disposable"));
        write.setTextData(QStringLiteral("MattOS QtKeychain roundtrip"));
        runJob(write);
        QKeychain::ReadPasswordJob read(service);
        read.setKey(QStringLiteral("disposable"));
        runJob(read);
        const bool matched = read.textData() == QStringLiteral("MattOS QtKeychain roundtrip");
        QKeychain::DeletePasswordJob remove(service);
        remove.setKey(QStringLiteral("disposable"));
        runJob(remove);
        QKeychain::ReadPasswordJob absent(service);
        absent.setKey(QStringLiteral("disposable"));
        runJob(absent, QKeychain::EntryNotFound);
        require(matched, "QtKeychain changed the stored secret");
        std::cout << "PASS QtKeychain secure KWallet write/read/delete" << std::endl;
    } catch (const std::exception &error) {
        std::cerr << error.what() << std::endl;
        return 1;
    }
}
"""
with tempfile.TemporaryDirectory(prefix='mattos-kde-') as tmp:
    cpp, binary = Path(tmp) / 'probe.cpp', Path(tmp) / 'probe'
    cpp.write_text(source)
    wav = Path(tmp) / 'tone.wav'
    with wave.open(str(wav), 'wb') as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(16000)
        output.writeframes(b''.join(struct.pack('<h', int(2048 * math.sin(2 * math.pi * 440 * sample / 16000))) for sample in range(4000)))
    flags = shlex.split(subprocess.check_output(['pkg-config', '--cflags', '--libs', 'Qt6Gui', 'Qt6Multimedia', 'tesseract', 'KF6PulseAudioQt'], text=True))
    subprocess.run(['g++', '-std=c++20', '-fPIC', str(cpp), '-o', str(binary),
                    '-I/usr/include/cxx-rust-cssparser', '-I/usr/include/union',
                    '-I/usr/include/union/Union',
                    '-lcxx-rust-cssparser', '-lUnion', '-lqt6keychain', *flags], check=True, timeout=90)
    env = dict(os.environ, QT_QPA_PLATFORM='offscreen')
    env.setdefault('XDG_RUNTIME_DIR', f'/run/user/{os.getuid()}')
    env.setdefault('DBUS_SESSION_BUS_ADDRESS', 'unix:path=' + env['XDG_RUNTIME_DIR'] + '/bus')
    subprocess.run([str(binary), 'mattos-validation-' + Path(tmp).name, str(wav)], env=env, check=True, timeout=90)
'''


def installed_upstream_probes() -> tuple[tuple[str, str], ...]:
    probes = []
    for name, script in (("glib-gio", GLIB), ("pixbuf-formats", PIXBUF),
                         ("xkb-keymaps", XKB), ("archive-formats", ARCHIVE),
                         ("gstreamer-pipeline", GSTREAMER), ("gnupg-signatures", GNUPG),
                         ("kde-stack-runtime", KDE_STACK)):
        encoded = base64.b64encode(zlib.compress(textwrap.dedent(script).encode())).decode()
        # QEMU's UART can lose a long paste before the guest shell receives it.
        # Upload bounded chunks in separate, acknowledged serial transactions.
        path = f"/tmp/mattos-upstream-{uuid4().hex}.payload"
        for index, start in enumerate(range(0, len(encoded), 480)):
            mode = "x" if index == 0 else "a"
            code = (f"from pathlib import Path; p=Path({path!r}); "
                    f"f=p.open({mode!r}); f.write({encoded[start:start + 480]!r}); f.close()")
            probes.append((f"{name}-transfer-{index + 1}", f"python3 -c {shlex.quote(code)}"))
        code = (f"import base64,zlib; from pathlib import Path; p=Path({path!r}); "
                "data=p.read_bytes(); p.unlink(); exec(zlib.decompress(base64.b64decode(data)))")
        # Union discovers plugins through the desktop's Qt environment. The
        # serial login does not inherit the user manager's environment.d files.
        runner = "systemd-run --user --wait --pipe --collect " if name == "kde-stack-runtime" else ""
        probes.append((name, f"timeout 180 {runner}python3 -c {shlex.quote(code)}"))
    return tuple(probes)
