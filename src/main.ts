import { mount } from "svelte";
import "@xterm/xterm/css/xterm.css";
import "./app.css";
import App from "./App.svelte";

const app = mount(App, { target: document.getElementById("app")! });
// 启动画面交给界面
document.getElementById("boot")?.remove();

export default app;
