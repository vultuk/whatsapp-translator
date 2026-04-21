import {
  renderComponentPreview,
  renderWorkspacePanel,
} from './whatsappTranslator.fixtures.js';

const meta = {
  title: 'Components/Workspace',
};

export default meta;

export const ConversationContext = {
  render: () => renderComponentPreview(renderWorkspacePanel({ reminderDue: true }), 'workspace'),
};

export const CollapsedState = {
  render: () => renderComponentPreview(renderWorkspacePanel({ collapsed: true, reminderDue: false }), 'workspace'),
};
