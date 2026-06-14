export const app = {
  getPath: (name: string) => './test-data/' + name,
  isPackaged: false,
}

export const BrowserWindow = class {}
export const ipcMain = {
  handle: () => {},
  on: () => {},
}
