// The receiving end of Apple's Credential Exchange: the Passwords app (or another manager)
// hands the items to this app in memory, and this turns them into the rows the Rust side
// already knows from the file importers. Nothing is logged; the JSON is handed to Rust and
// freed there.

import AuthenticationServices
import Foundation

@available(macOS 26.0, *)
struct ImportedRow: Encodable {
    var kind: String
    var origin: String
    var username: String
    var password: String?
    var otpauth: String?
    var passkey: ImportedPasskey?
    var label: String
    var notes: String
}

// A passkey the exporter offered. Its private key is deliberately NOT carried across:
// this app keeps passkey keys on the server, where they are used in the bot's browser, and
// a key that reached the Mac would have nowhere to live. Until the server takes a key by
// this road, a passkey item is counted and named, not imported.
@available(macOS 26.0, *)
struct ImportedPasskey: Encodable {
    var credentialId: String   // base64url
    var rpId: String
    var userName: String
    var userHandle: String     // base64url
}

@available(macOS 26.0, *)
enum ImportReceiver {
    /// Pull everything the exporter offered under this token.
    static func importAll(token: UUID) async throws -> [ImportedRow] {
        let data = try await ASCredentialImportManager().importCredentials(token: token)
        var collected: [ImportedRow] = []
        for account in data.accounts {
            for item in account.items {
                collected.append(contentsOf: rows(for: item))
            }
        }
        return collected
    }

    private static func rows(for item: ASImportableItem) -> [ImportedRow] {
        let origin = item.scope?.urls.first.flatMap { registrableHost($0) } ?? ""
        var password: String?
        var username = ""
        var otpauth: String?
        var passkey: ImportedPasskey?
        var notes: [String] = []
        for credential in item.credentials {
            switch credential {
            case .basicAuthentication(let basic):
                username = basic.userName?.value ?? username
                password = basic.password?.value
            case .totp(let totp):
                otpauth = otpauthURI(totp, fallbackName: username, title: item.title)
                if username.isEmpty, let name = totp.userName { username = name }
            case .passkey(let key):
                passkey = ImportedPasskey(
                    credentialId: key.credentialID.base64URLEncodedString(),
                    rpId: key.relyingPartyIdentifier,
                    userName: key.userName,
                    userHandle: key.userHandle.base64URLEncodedString()
                )
                if username.isEmpty { username = key.userName }
            case .note(let note):
                let text = note.content.value
                if !text.isEmpty { notes.append(text) }
            default:
                continue
            }
        }
        let site = origin.isEmpty ? (passkey?.rpId ?? "") : origin
        guard !site.isEmpty, !username.isEmpty else { return [] }
        let kind: String
        if passkey != nil { kind = "passkey" } else if password == nil, otpauth != nil { kind = "code" } else if password != nil { kind = "password" } else { return [] }
        return [ImportedRow(
            kind: kind, origin: site, username: username, password: password, otpauth: otpauth,
            passkey: passkey, label: item.title, notes: notes.joined(separator: "\n")
        )]
    }

    /// The seed as an `otpauth://` URI, the shape every other importer produces.
    private static func otpauthURI(_ totp: ASImportableCredential.TOTP, fallbackName: String, title: String) -> String {
        let secret = base32(totp.secret)
        let algorithm: String
        switch totp.algorithm {
        case .sha256: algorithm = "SHA256"
        case .sha512: algorithm = "SHA512"
        default: algorithm = "SHA1"
        }
        let name = totp.userName ?? fallbackName
        let issuer = totp.issuer ?? title
        let label = (issuer.isEmpty ? name : "\(issuer):\(name)")
            .addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? name
        var uri = "otpauth://totp/\(label)?secret=\(secret)&digits=\(totp.digits)&period=\(totp.period)&algorithm=\(algorithm)"
        if !issuer.isEmpty, let encoded = issuer.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) {
            uri += "&issuer=\(encoded)"
        }
        return uri
    }

    /// The host of a URL, lowercase, without a leading `www.`. The Rust side reduces it to
    /// the registrable site like every other importer's rows.
    private static func registrableHost(_ url: URL) -> String? {
        guard let host = url.host()?.lowercased(), !host.isEmpty else { return nil }
        return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
    }

    private static func base32(_ data: Data) -> String {
        let alphabet = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZ234567")
        var bits = 0, value = 0, out = ""
        for byte in data {
            value = (value << 8) | Int(byte)
            bits += 8
            while bits >= 5 {
                out.append(alphabet[(value >> (bits - 5)) & 31])
                bits -= 5
            }
        }
        if bits > 0 { out.append(alphabet[(value << (5 - bits)) & 31]) }
        return out
    }
}

extension Data {
    func base64URLEncodedString() -> String {
        base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}
