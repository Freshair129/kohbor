import { mountAccountSlice } from './account-slice.mjs';
import { createFakeAccountController } from './fake-account.mjs';

const root = document.getElementById('s1c-root');
const controller = createFakeAccountController();
mountAccountSlice(root, controller);
