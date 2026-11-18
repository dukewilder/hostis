// Whether this device's own traffic goes through a VPN, for the duties a device takes on by
// choice that put its address in front of strangers: a spare phone on all day, links that name
// this radio, and a computer offering the most it can. Ordinary listening and carrying need no
// VPN and ask nothing of this file.
//
// A phone answers from Android itself: the app's own active network carries TRANSPORT_VPN, so a
// VPN that leaves this app out counts as none. A computer has no such answer, so it looks for a
// VPN's adapter holding the default route, and anything it cannot read counts as no VPN.
import 'dart:io';

import 'package:flutter/material.dart';

import 'main.dart';
import 'native.dart';
import 'shell.dart';

/// Refused on a phone, turning on the spare phone with no VPN and no Tor.
const spareNeedsVpn = 'Turn on a VPN first. A spare phone is on all day for strangers to reach, so it shows its address. With a VPN they see the VPN\'s address instead.';

/// Refused on a phone, turning on links that name this radio with no VPN and no Tor.
const linksNeedVpn = 'Turn on a VPN first. Links that name this radio carry its address wherever they travel. With a VPN they carry the VPN\'s address instead.';

/// Asked on a computer before a choice that leaves it reachable at its own address.
const computerNoVpn = 'No VPN found. This computer will be reachable by strangers at its own address. Turn this on only if you have a VPN running.';

/// Whether an adapter's name is a VPN's: tun, wg, utun, ppp and tap devices, and the adapters
/// the usual VPN apps install under their own names.
bool vpnAdapter(String name) {
  final n = name.trim().toLowerCase();
  if (n.isEmpty) return false;
  if (RegExp(r'^(tun|wg|utun|ppp|tap)[0-9a-z_.-]*$').hasMatch(n)) return true;
  return ['wireguard', 'proton', 'openvpn', 'mullvad', 'nordlynx', 'tap-windows', 'wintun', 'vpn'].any(n.contains);
}

/// The adapter holding the IPv4 default route, read from Linux's /proc/net/route.
String? linuxDefault(String route) {
  String? best;
  int bestMetric = 1 << 30;
  for (final line in route.split('\n').skip(1)) {
    final f = line.trim().split(RegExp(r'\s+'));
    if (f.length < 8 || f[1] != '00000000' || f[7] != '00000000') continue;
    final metric = int.tryParse(f[6]) ?? 0;
    if (metric < bestMetric) {
      best = f[0];
      bestMetric = metric;
    }
  }
  return best;
}

/// The adapter a Mac's `route -n get default` names.
String? macDefault(String out) {
  final m = RegExp(r'^\s*interface:\s*(\S+)', multiLine: true).firstMatch(out);
  return m?.group(1);
}

/// A spare phone or links naming this radio may go on, or stay on, with a VPN or through Tor.
bool dedicationAllowed({required bool vpn, required bool tor}) => vpn || tor;

/// Whether the radio goes through its own Tor tunnel.
bool torOn() => ((Native.call({'cmd': 'tor'}) as Map?)?['url'] ?? '').toString().isNotEmpty;

class Vpn {
  static const _channel = ShellChannel();

  /// Whether this device's own traffic goes through a VPN. Unsure is no.
  static Future<bool> on() async => await _channel.invokeMethod<bool>('vpn').catchError((_) => false) ?? false;

  /// A computer's best guess: the adapter holding the default route looks like a VPN's.
  static Future<bool> desktop() async {
    try {
      String? name;
      if (Platform.isLinux) {
        name = linuxDefault(await File('/proc/net/route').readAsString());
      } else if (Platform.isMacOS) {
        final r = await Process.run('/sbin/route', ['-n', 'get', 'default']);
        name = macDefault(r.stdout.toString());
      } else if (Platform.isWindows) {
        // The default route's adapter, by its name and its description (where WireGuard,
        // Proton and the rest say what they are).
        final r = await Process.run('powershell', [
          '-NoProfile', '-NonInteractive', '-Command',
          r'$r = Get-NetRoute -DestinationPrefix 0.0.0.0/0 | Sort-Object RouteMetric | Select-Object -First 1; $a = Get-NetAdapter -InterfaceIndex $r.InterfaceIndex; $a.Name; $a.InterfaceDescription',
        ]);
        if (r.exitCode != 0) return false;
        return r.stdout.toString().split('\n').any(vpnAdapter);
      }
      return name != null && vpnAdapter(name);
    } catch (_) {
      return false;
    }
  }

  /// On a phone: true if the duty may go on, and says why not otherwise.
  static Future<bool> phoneGate(String refusal) async {
    if (dedicationAllowed(vpn: await on(), tor: torOn())) return true;
    say(refusal);
    return false;
  }

  /// On a computer: true if a VPN is found, or its owner says there is one.
  static Future<bool> computerGate(BuildContext context) async {
    if (await desktop()) return true;
    if (!context.mounted) return false;
    final yes = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        backgroundColor: panel,
        shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
        content: Text(computerNoVpn, style: serif(15)),
        actions: [
          TextButton(onPressed: () => Navigator.pop(c, false), child: Text('CANCEL', style: carved(11, color: boneFaint))),
          TextButton(onPressed: () => Navigator.pop(c, true), child: Text('I HAVE ONE', style: carved(11))),
        ],
      ),
    );
    return yes == true;
  }
}
