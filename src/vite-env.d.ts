/// <reference types="vite/client" />

declare module "*.css" {
  const content: string;
  export default content;
}

declare module "@fontsource-variable/geist" {}
declare module "@fontsource-variable/inclusive-sans" {}
