import SwiftUI

struct TranslatorPalette: Equatable {
    let accent: Color
    let deepAccent: Color
    let incomingBubble: Color
    let outgoingBubble: Color
    let chatBackground: Color
    let backdropSecondary: Color

    static func make(_ theme: AppTheme) -> TranslatorPalette {
        let colors: (String, String, String, String, String, String, String, String)
        switch theme {
        case .whatsapp: colors = ("00A884", "00695C", "FFFFFF", "1F2C33", "D9FDD3", "005C4B", "EFEAE2", "0B141A")
        case .ocean: colors = ("0784C6", "075985", "FFFFFF", "172554", "DDF3FF", "164E63", "EDF8FF", "071827")
        case .sunset: colors = ("F26B4A", "B9382F", "FFFDFC", "36201F", "FFE1D6", "79362F", "FFF2E8", "1E1316")
        case .github: colors = ("238636", "1A7F37", "FFFFFF", "161B22", "DDF4E4", "1F6F32", "F6F8FA", "0D1117")
        case .dracula: colors = ("BD93F9", "8E64CC", "FFFFFF", "282A36", "E8DEFF", "44475A", "F4F0FA", "1E1F29")
        case .nord: colors = ("5E81AC", "3B5B7D", "FFFFFF", "2E3440", "D8DEE9", "4C566A", "ECEFF4", "242933")
        case .linear: colors = ("7C5CFC", "5E43D8", "FFFFFF", "202026", "E9E3FF", "454052", "F7F5FA", "151518")
        case .vercel: colors = ("111111", "000000", "FFFFFF", "1A1A1A", "EAEAEA", "333333", "F5F5F5", "0A0A0A")
        }
        return TranslatorPalette(
            accent: Color(hex: colors.0),
            deepAccent: Color(hex: colors.1),
            incomingBubble: .adaptive(light: colors.2, dark: colors.3),
            outgoingBubble: .adaptive(light: colors.4, dark: colors.5),
            chatBackground: .adaptive(light: colors.6, dark: colors.7),
            backdropSecondary: theme == .sunset ? .orange : (theme == .ocean ? .cyan : .indigo)
        )
    }
}

private struct TranslatorPaletteKey: EnvironmentKey {
    static let defaultValue = TranslatorPalette.make(.whatsapp)
}

extension EnvironmentValues {
    var translatorPalette: TranslatorPalette {
        get { self[TranslatorPaletteKey.self] }
        set { self[TranslatorPaletteKey.self] = newValue }
    }
}

struct TranslatorBackdrop: View {
    @Environment(\.translatorPalette) private var palette

    var body: some View {
        ZStack {
            Color(uiColor: .systemBackground)
            Circle()
                .fill(palette.accent.opacity(0.20))
                .frame(width: 420, height: 420)
                .blur(radius: 80)
                .offset(x: -180, y: -280)
            Circle()
                .fill(palette.backdropSecondary.opacity(0.12))
                .frame(width: 360, height: 360)
                .blur(radius: 90)
                .offset(x: 190, y: 330)
        }
        .ignoresSafeArea()
    }
}

struct TranslatorMark: View {
    @Environment(\.translatorPalette) private var palette
    let size: CGFloat

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
                .fill(palette.accent.gradient)
            Image(systemName: "captions.bubble.fill")
                .font(.system(size: size * 0.46, weight: .semibold))
                .foregroundStyle(.white)
        }
        .frame(width: size, height: size)
        .shadow(color: palette.deepAccent.opacity(0.18), radius: 22, y: 12)
    }
}

extension View {
    @ViewBuilder
    func translatorGlass<S: Shape>(in shape: S) -> some View {
        if #available(iOS 26.0, *) {
            glassEffect(.regular, in: shape)
        } else {
            background(.ultraThinMaterial, in: shape)
                .overlay(shape.stroke(.white.opacity(0.22), lineWidth: 0.5))
        }
    }
}

private extension Color {
    init(hex: String) {
        let value = UInt64(hex, radix: 16) ?? 0
        self.init(
            red: Double((value >> 16) & 0xFF) / 255,
            green: Double((value >> 8) & 0xFF) / 255,
            blue: Double(value & 0xFF) / 255
        )
    }

    static func adaptive(light: String, dark: String) -> Color {
        Color(uiColor: UIColor { traits in
            UIColor(hex: traits.userInterfaceStyle == .dark ? dark : light)
        })
    }
}

private extension UIColor {
    convenience init(hex: String) {
        let value = UInt64(hex, radix: 16) ?? 0
        self.init(
            red: CGFloat((value >> 16) & 0xFF) / 255,
            green: CGFloat((value >> 8) & 0xFF) / 255,
            blue: CGFloat(value & 0xFF) / 255,
            alpha: 1
        )
    }
}
