# Tests of malformed DNS packets
# Copyright (C) Catalyst.NET ltd
#
# written by Douglas Bagnall <douglas.bagnall@catalyst.net.nz>
#
# This program is free software; you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation; either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

"""Sanity tests for DNS and NBT server parsing.

We don't use a proper client library so we can make improper packets.
"""

import os
import sys
import struct
import socket
import select
import time
from samba.dcerpc import dns, nbt
from samba.tests import TestCase


def _msg_id():
    while True:
        for i in range(1, 0xffff):
            yield i


SERVER = os.environ['SERVER_IP']
SERVER_NAME = "%s.%s" % (os.environ['SERVER'], os.environ['REALM'])
NBT_NAME = 'EOGFGLGPCACACACACACACACACACACACA'  # "neko"
TIMEOUT = 0.5

VERBOSE = '-v' in sys.argv

# We use OK for rcode assertions when we don't known whether the query
# is DNS or NBT (not exactly coincidentally, OK is 0 in both cases).
OK = dns.DNS_RCODE_OK


def encode_netbios_bytes(chars):
    """RFC 1002 calls this "first-level encoding"."""
    out = []
    chars = (chars + b'                   ')[:16]
    for c in chars:
        out.append((c >> 4) + 65)
        out.append((c & 15) + 65)
    return bytes(out)


class TestDnsPacketBase(TestCase):
    msg_id = _msg_id()

    def tearDown(self):
        # we need to ensure the DNS server is responsive before
        # continuing. This will catch the return of any DoS problems
        # like CVE-2020-10745.
        ok = self._known_good_query()
        if not ok:
            self.fail("the server is STILL unresponsive")

    def decode_reply(self, data):
        header = data[:12]
        id, flags, n_q, n_a, n_rec, n_exta = struct.unpack('!6H',
                                                           header)
        return {
            'rcode': flags & 0xf
        }

    def construct_query(self, names):
        """Create a query packet containing one query record.

        *names* is either a single string name in the usual dotted
        form, or a list of names. In the latter case, each name can
        be a dotted string or a list of byte components, which allows
        dots in components. Where I say list, I mean non-string
        iterable.

        Examples:

        # these 3 are all the same
        "example.com"
        ["example.com"]
        [[b"example", b"com"]]

        # this is three names in the same request
        ["example.com",
         [b"example", b"com", b"..!"],
         (b"first component", b" 2nd component")]
        """
        header = struct.pack('!6H',
                             next(self.msg_id),
                             0x0100,       # query, with recursion
                             len(names),   # number of queries
                             0x0000,       # no answers
                             0x0000,       # no records
                             0x0000,       # no extra records
        )
        tail = struct.pack('!BHH',
                           0x00,         # root node
                           self.qtype,
                           0x0001,       # class IN-ternet
        )
        encoded_bits = []
        for name in names:
            if isinstance(name, str):
                bits = name.encode('utf8').split(b'.')
            else:
                bits = name

            for b in bits:
                encoded_bits.append(b'%c%s' % (len(b), b))
            encoded_bits.append(tail)

        return header + b''.join(encoded_bits)

    def _test_query(self, names=(), expected_rcode=None):
        if isinstance(names, str):
            names = [names]

        packet = self.construct_query(names)
        start = time.time()
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.sendto(packet, self.server)
        r, _, _ = select.select([s], [], [], TIMEOUT)

        # For some queries Windows varies its response. In these
        # cases, we accept any of the known replies, or noreply if it
        # is an option.
        if (isinstance(expected_rcode, set) and
            None in expected_rcode and
            r == []):
            expected_rcode = None

        if expected_rcode is None:
            # we don't think the server should answer (the packet is
            # rubbish), but we also want to be sure that the reason
            # the server is not answering is not that it is stuck
            # doing work on the packet (c.f. CVE-2020-10745). So we
            # wait for a while, then immediately send another, easy
            # packet. If the reply to the easy packet comes back
            # quickly, we say the server is good.

            elapsed = time.time() - start
            if r:
                data, addr = s.recvfrom(16 * 1024)
                s.close()

                rcode = self.decode_reply(data)['rcode']
                self.fail(
                    "an answer was not expected "
                    "(timeout {}, elapsed {:.2f}, rcode {})".format(TIMEOUT,
                                                                    elapsed,
                                                                    rcode))

            s.close()

            self.assertTrue(self._known_good_query(),
                            "The query timed out (good), but the server is "
                            "still unresponsive.")

        else:
            if r != [s]:
                s.close()
                self.fail("an answer was expected within %s seconds)" % TIMEOUT)

            data, addr = s.recvfrom(16 * 1024)
            s.close()
            rcode = self.decode_reply(data)['rcode']

            if isinstance(expected_rcode, set):
                self.assertIn(rcode,
                              expected_rcode,
                              "expected RCODE %s, got %s" % (expected_rcode,
                                                             rcode))
            else:
                self.assertEqual(rcode,
                                 expected_rcode,
                                 "expected RCODE %s, got %s" % (expected_rcode,
                                                                rcode))

            if VERBOSE:
                print(data, len(data))
        if VERBOSE:
            print("succeeded in %f seconds" % (time.time() - start))

    def _known_good_query(self):
        if self.server[1] == 53:
            name = SERVER_NAME
            expected_rcode = dns.DNS_RCODE_OK
        else:
            name = [encode_netbios_bytes(b'nxdomain'), b'nxdomain']
            expected_rcode = nbt.NBT_RCODE_NAM

        packet = self.construct_query([name])
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.sendto(packet, self.server)
        r, _, _ = select.select([s], [], [], TIMEOUT)
        if r != [s]:
            s.close()
            return False

        data, addr = s.recvfrom(4096)
        s.close()
        rcode = self.decode_reply(data)['rcode']
        return expected_rcode == rcode

    def _test_empty_packet(self):

        packet = b""
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.sendto(packet, self.server)
        s.close()

        # It is reasonable not to reply to an empty packet
        # but it is not reasonable to render the server
        # unresponsive.
        ok = self._known_good_query()
        self.assertTrue(ok, "the server is unresponsive")

    def _make_long_name(self, length, first_component=None):
        name = []
        if first_component is not None:
            name.append(first_component)
            length -= len(first_component) + 1
        while length > 33:
            name.append("x" * 30)
            length -= 31
        name.append("x" * length)
        return '.'.join(name)


class TestDnsPackets(TestDnsPacketBase):
    server = (SERVER, 53)
    qtype = 1     # dns type A

    def _test_many_repeated_components(self, label, n, expected_rcode=None):
        name = [label] * n
        self._test_query([name],
                         expected_rcode=expected_rcode)

    def test_127_very_dotty_components(self):
        label = b'.' * 63
        # Windows will refuse to reply, but we also accept format error
        self._test_many_repeated_components(label, 127,
                                            expected_rcode={None,
                                                            nbt.NBT_RCODE_FMT})

    def test_127_half_dotty_components(self):
        label = b'x.' * 31 + b'x'
        # Windows will refuse to reply
        self._test_many_repeated_components(label, 127,
                                            expected_rcode={None,
                                                            nbt.NBT_RCODE_FMT})

    def test_253_bytes(self):
        name = self._make_long_name(253)
        self._test_query(name,
                         expected_rcode=dns.DNS_RCODE_NXDOMAIN)

    def test_254_bytes(self):
        name = self._make_long_name(254)
        self._test_query(name,
                         expected_rcode=dns.DNS_RCODE_FORMERR)

    def test_empty_packet(self):
        self._test_empty_packet()


class TestNbtPackets(TestDnsPacketBase):
    server = (SERVER, 137)
    qtype = 0x20  # NBT_QTYPE_NETBIOS

    def _test_many_repeated_components(self, label, n,
                                       expected_rcode=None):
        name = [label] * n
        name[0] = encode_netbios_bytes(label)
        self._test_query([name],
                         expected_rcode=expected_rcode)

    def _test_nbt_encode_query(self, names, *args, **kwargs):
        if isinstance(names, str):
            names = [names]

        nbt_names = []
        for name in names:
            if isinstance(name, str):
                bits = name.encode('utf8').split(b'.')
            else:
                bits = name

            encoded = [encode_netbios_bytes(bits[0])]
            encoded.extend(bits[1:])
            nbt_names.append(encoded)

        self._test_query(nbt_names, *args, **kwargs)

    def test_127_very_dotty_components(self):
        label = b'.' * 63
        # Windows will refuse to reply
        self._test_many_repeated_components(label, 127,
                                            expected_rcode={None,
                                                            nbt.NBT_RCODE_FMT})

    def test_127_half_dotty_components(self):
        label = b'x.' * 31 + b'x'
        # Windows will refuse to reply
        self._test_many_repeated_components(label, 127,
                                            expected_rcode={None,
                                                            nbt.NBT_RCODE_FMT})

    def test_empty_packet(self):
        self._test_empty_packet()

    def test_253_bytes(self):
        name = self._make_long_name(253, NBT_NAME)
        self._test_query(name,
                         expected_rcode=nbt.NBT_RCODE_NAM)

    def test_254_bytes(self):
        # This works because we follow Windows, not RFC 1001/1002.
        # (see next test for a longer explanation).
        name = self._make_long_name(254, NBT_NAME)
        self._test_query(name,
                         expected_rcode=nbt.NBT_RCODE_NAM)

    def test_272_bytes(self):
        # This works because we (contra RFC, following Windows) treat
        # the 32 byte encoded Netbios name as if it were the 16 byte
        # un-encoded form (used in MS-WINSRA), AND add three bytes
        # because what kind of limit is 253 anyway? (matches 2012r2)
        name = self._make_long_name(272, NBT_NAME)
        self._test_query(name,
                         expected_rcode=nbt.NBT_RCODE_NAM)

    def test_273_bytes(self):
        # Finally we exhaust Windows' generosity toward long names.
        name = self._make_long_name(273, NBT_NAME)
        self._test_query(name,
                         expected_rcode={None, nbt.NBT_RCODE_FMT})
