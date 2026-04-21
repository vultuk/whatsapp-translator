import {
  renderComponentPreview,
  renderContactItem,
  renderContactsPanel,
  renderVisitorDashboard,
} from './whatsappTranslator.fixtures.js';

const meta = {
  title: 'Components/Inbox',
};

export default meta;

export const ContactRail = {
  render: () => renderComponentPreview(renderContactsPanel({ activeId: 'jules' }), 'sidebar'),
};

export const NeedsReplyRow = {
  render: () => renderComponentPreview(renderContactItem({
    id: 'host',
    name: 'Sofia at Casa Azul',
    initial: 'S',
    preview: 'Check-in is ready for 18:30. Please send the taxi plate when you have it.',
    time: '09:18',
    unread: 2,
    pinned: true,
    badges: [
      { type: 'priority urgent', label: 'Urgent' },
      { type: 'reply', label: 'Reply' },
      { type: 'label', label: 'Travel' },
    ],
  }), 'sidebar'),
};

export const DashboardSummary = {
  render: () => renderComponentPreview(renderVisitorDashboard(), 'chat'),
};
