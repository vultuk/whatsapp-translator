import {
  renderComponentPreview,
  renderMessagesList,
} from './whatsappTranslator.fixtures.js';

const meta = {
  title: 'Components/Messages',
};

export default meta;

export const ConversationThread = {
  render: () => renderComponentPreview(renderMessagesList(), 'chat'),
};

export const IncomingTranslationTooltip = {
  render: () => renderComponentPreview(renderMessagesList([
    {
      sender: 'Sofia',
      body: 'The keys are in the lockbox by the blue gate.',
      time: '09:21',
      translated: true,
      translationOpen: true,
      translationText: 'Les clefs sont dans la boite a code pres du portail bleu.',
      translationLanguage: 'French',
      reactions: [{ emoji: '🙏', count: 1 }],
    },
  ]), 'chat'),
};

export const OutgoingReply = {
  render: () => renderComponentPreview(renderMessagesList([
    {
      outgoing: true,
      quoted: {
        sender: 'Sofia',
        text: 'Can I arrive after 18:00?',
      },
      body: '18:30 works on our side. Please send the taxi plate when you have it.',
      time: '09:24',
      starred: true,
    },
  ]), 'chat'),
};
