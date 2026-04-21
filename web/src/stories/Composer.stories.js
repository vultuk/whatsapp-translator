import {
  renderComponentPreview,
  renderComposer,
} from './whatsappTranslator.fixtures.js';

const meta = {
  title: 'Components/Composer',
};

export default meta;

export const DraftAndReply = {
  render: () => renderComponentPreview(renderComposer({ showReply: true, showDraft: true }), 'composer'),
};

export const CleanComposer = {
  render: () => renderComponentPreview(renderComposer({ showReply: false, showDraft: false }), 'composer'),
};
