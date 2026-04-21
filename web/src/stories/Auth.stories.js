import { renderAuthOverlay } from './whatsappTranslator.fixtures.js';

const meta = {
  title: 'Components/Auth',
  render: (args) => renderAuthOverlay(args),
};

export default meta;

export const PasswordGate = {
  args: {
    showError: false,
  },
};

export const InvalidPassword = {
  args: {
    showError: true,
  },
};
