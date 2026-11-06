import Foundation
import CoreGraphics
import ImageIO

let icons = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
    .appendingPathComponent("src-tauri/icons")
let names = [
    "32x32.png", "64x64.png", "128x128.png", "128x128@2x.png", "icon.png", "icon.ico", "icon.icns",
    "Square30x30Logo.png", "Square44x44Logo.png", "Square71x71Logo.png", "Square89x89Logo.png",
    "Square107x107Logo.png", "Square142x142Logo.png", "Square150x150Logo.png", "Square284x284Logo.png",
    "Square310x310Logo.png", "StoreLogo.png",
]
for name in names {
    guard let source = CGImageSourceCreateWithURL(icons.appendingPathComponent(name) as CFURL, nil) else {
        fatalError("Cannot read \(name)")
    }
    for index in 0..<CGImageSourceGetCount(source) {
        guard let image = CGImageSourceCreateImageAtIndex(source, index, nil) else { fatalError("Cannot decode \(name)") }
        let width = image.width, height = image.height
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let bitmap = CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue
        pixels.withUnsafeMutableBytes { ptr in
            let context = CGContext(data: ptr.baseAddress, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: bitmap)!
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        }
        func alpha(_ x: Int, _ y: Int) -> UInt8 { pixels[(y * width + x) * 4 + 3] }
        let corners = [alpha(0, 0), alpha(width-1, 0), alpha(0, height-1), alpha(width-1, height-1)]
        guard corners.allSatisfy({ $0 == 0 }) && alpha(width/2, height/2) > 0 else { fatalError("Invalid transparency in \(name)") }
        if name == "icon.icns" && width >= 128 {
            var minX = width, maxX = -1, minY = height, maxY = -1
            for y in 0..<height { for x in 0..<width where alpha(x, y) > 0 { minX = min(minX,x); maxX = max(maxX,x); minY = min(minY,y); maxY = max(maxY,y) } }
            let margins = [minX, minY, width-1-maxX, height-1-maxY]
            guard margins.allSatisfy({ Double($0) / Double(width) >= 0.09 && Double($0) / Double(width) <= 0.11 }) else {
                fatalError("Invalid macOS icon margins at \(width)x\(height)")
            }
        }
        print("\(name) \(width)x\(height): ok")
    }
}
