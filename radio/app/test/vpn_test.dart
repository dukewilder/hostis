// The VPN gate for a device's dedicated duties: which adapters count as a VPN's, which adapter
// holds the default route, and that a spare phone or links naming the radio go on only with a
// VPN or Tor.
import 'package:flutter_test/flutter_test.dart';
import 'package:pyrate/vpn.dart';

void main() {
  test('VPN adapters are known by name', () {
    for (final n in ['tun0', 'wg0', 'utun3', 'ppp0', 'tap1', 'WireGuard Tunnel', 'ProtonVPN', 'Mullvad', 'nordlynx', 'Wintun Userspace Tunnel']) {
      expect(vpnAdapter(n), isTrue, reason: n);
    }
    for (final n in ['eth0', 'wlan0', 'en0', 'enp3s0', 'Wi-Fi', 'Ethernet', 'Intel(R) Wi-Fi 6 AX201', '', 'lo']) {
      expect(vpnAdapter(n), isFalse, reason: n);
    }
  });

  test('the default route is read from /proc/net/route', () {
    const route = 'Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n'
        'wlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n'
        'wg0\t00000000\t00000000\t0001\t0\t0\t50\t00000000\t0\t0\t0\n'
        'wlan0\t0001A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\t0\t0\t0\n';
    expect(linuxDefault(route), 'wg0');
    expect(linuxDefault(route.replaceAll('wg0\t00000000\t00000000\t0001\t0\t0\t50', 'wg0\t0000000A\t00000000\t0001\t0\t0\t50')), 'wlan0');
    expect(linuxDefault('Iface\tDestination\n'), isNull);
  });

  test('a Mac names the default route\'s interface', () {
    expect(macDefault('   route to: default\ndestination: default\n    gateway: 10.2.0.1\n  interface: utun4\n'), 'utun4');
    expect(macDefault('route: writing to routing socket: not in table'), isNull);
  });

  test('a dedicated duty needs a VPN or Tor', () {
    expect(dedicationAllowed(vpn: false, tor: false), isFalse);
    expect(dedicationAllowed(vpn: true, tor: false), isTrue);
    expect(dedicationAllowed(vpn: false, tor: true), isTrue);
  });

  test('the refusals say it plainly', () {
    for (final m in [spareNeedsVpn, linksNeedVpn, computerNoVpn]) {
      expect(m.contains('—'), isFalse);
    }
    expect(spareNeedsVpn, 'Turn on a VPN first. A spare phone is on all day for strangers to reach, so it shows its address. With a VPN they see the VPN\'s address instead.');
  });
}
