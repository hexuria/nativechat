// The C ABI the Rust app calls once the activity with the import token arrives.
// `nc_credential_exchange_import` returns a JSON array of rows (owned C string) or NULL
// with the error in `error_out`; both are freed with `nc_credential_exchange_free`.

import Foundation

@available(macOS 26.0, *)
@_cdecl("nc_credential_exchange_import")
public func nc_credential_exchange_import(
    _ token: UnsafePointer<CChar>,
    _ errorOut: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> UnsafeMutablePointer<CChar>? {
    guard let uuid = UUID(uuidString: String(cString: token)) else {
        errorOut.pointee = strdup("the import token is not a UUID")
        return nil
    }
    let group = DispatchGroup()
    var result: Result<[ImportedRow], Error>?
    group.enter()
    Task {
        do { result = .success(try await ImportReceiver.importAll(token: uuid)) }
        catch { result = .failure(error) }
        group.leave()
    }
    group.wait()
    switch result {
    case .success(let rows):
        do {
            let json = try JSONEncoder().encode(rows)
            return json.withUnsafeBytes { buffer -> UnsafeMutablePointer<CChar>? in
                let out = UnsafeMutablePointer<CChar>.allocate(capacity: buffer.count + 1)
                buffer.copyBytes(to: UnsafeMutableRawBufferPointer(start: out, count: buffer.count))
                out[buffer.count] = 0
                return out
            }
        } catch {
            errorOut.pointee = strdup("could not encode the rows: \(error)")
            return nil
        }
    case .failure(let error):
        errorOut.pointee = strdup("the exchange failed: \(error.localizedDescription)")
        return nil
    case .none:
        errorOut.pointee = strdup("the exchange gave no answer")
        return nil
    }
}

@_cdecl("nc_credential_exchange_free")
public func nc_credential_exchange_free(_ pointer: UnsafeMutablePointer<CChar>?) {
    guard let pointer else { return }
    pointer.deallocate()
}
