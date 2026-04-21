import {
  renderConversationLayout,
  renderInboxLayout,
} from './whatsappTranslator.fixtures.js';

const meta = {
  title: 'Layouts',
  parameters: {
    pageLayout: 'page',
  },
};

export default meta;

export const InboxOverview = {
  render: () => renderInboxLayout(),
};

export const ActiveConversation = {
  render: () => renderConversationLayout(),
};

export const ActiveConversationMobile = {
  render: () => renderConversationLayout({ mobileWorkspaceOpen: true }),
  globals: {
    viewport: { value: 'iphone14promax', isRotated: false },
  },
};
