import SwiftUI

enum TranslatorTheme {
    static let green = Color(red: 0.0, green: 0.66, blue: 0.52)
    static let deepGreen = Color(red: 0.0, green: 0.37, blue: 0.29)
    static let incomingBubble = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 0.12, green: 0.17, blue: 0.19, alpha: 1)
            : UIColor.white
    })
    static let outgoingBubble = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 0.0, green: 0.35, blue: 0.27, alpha: 1)
            : UIColor(red: 0.84, green: 0.97, blue: 0.80, alpha: 1)
    })
    static let chatBackground = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 0.04, green: 0.08, blue: 0.10, alpha: 1)
            : UIColor(red: 0.94, green: 0.92, blue: 0.88, alpha: 1)
    })
}

struct TranslatorBackdrop: View {
    var body: some View {
        ZStack {
            Color(uiColor: .systemBackground)
            Circle()
                .fill(TranslatorTheme.green.opacity(0.20))
                .frame(width: 420, height: 420)
                .blur(radius: 80)
                .offset(x: -180, y: -280)
            Circle()
                .fill(Color.cyan.opacity(0.12))
                .frame(width: 360, height: 360)
                .blur(radius: 90)
                .offset(x: 190, y: 330)
        }
        .ignoresSafeArea()
    }
}

struct TranslatorMark: View {
    let size: CGFloat

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
                .fill(TranslatorTheme.green.gradient)
            Image(systemName: "captions.bubble.fill")
                .font(.system(size: size * 0.46, weight: .semibold))
                .foregroundStyle(.white)
        }
        .frame(width: size, height: size)
        .shadow(color: TranslatorTheme.deepGreen.opacity(0.18), radius: 22, y: 12)
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
