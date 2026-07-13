import SwiftUI

struct ContactAvatar: View {
    let contact: Contact
    let url: URL?
    var size: CGFloat = 52

    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image
                        .resizable()
                        .scaledToFill()
                default:
                    Circle()
                        .fill(contact.isGroup ? Color.indigo.opacity(0.16) : TranslatorTheme.green.opacity(0.14))
                        .overlay {
                            Text(contact.initials)
                                .font(.system(size: size * 0.34, weight: .semibold))
                                .foregroundStyle(contact.isGroup ? .indigo : TranslatorTheme.deepGreen)
                        }
                }
            }
            .frame(width: size, height: size)
            .clipShape(Circle())

            if contact.isGroup {
                Image(systemName: "person.2.fill")
                    .font(.system(size: max(7, size * 0.15), weight: .bold))
                    .foregroundStyle(.white)
                    .padding(max(4, size * 0.09))
                    .background(TranslatorTheme.green, in: Circle())
            }
        }
        .frame(width: size, height: size)
    }
}
