// Runs as a separate process so cross-process taskbar parenting cannot reset
// the main application's DPI awareness or block its UI thread.
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Drawing.Text;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Automation;
using System.Windows.Forms;
using Microsoft.Win32;

internal sealed class WalletContext : ApplicationContext {
    [StructLayout(LayoutKind.Sequential)] struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] struct POINT {
        public int X, Y;
        public POINT(int x, int y) { X = x; Y = y; }
    }
    [StructLayout(LayoutKind.Sequential)] struct SIZE {
        public int Width, Height;
        public SIZE(int width, int height) { Width = width; Height = height; }
    }
    [StructLayout(LayoutKind.Sequential, Pack = 1)] struct BLEND {
        public byte Op, Flags, Alpha, Format;
    }
    delegate bool EnumCallback(IntPtr window, IntPtr unused);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder name, int length);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumCallback callback, IntPtr unused);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out RECT rect);
    [DllImport("user32.dll")] static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern IntPtr SetParent(IntPtr window, IntPtr parent);
    [DllImport("user32.dll")] static extern IntPtr GetParent(IntPtr window);
    [DllImport("user32.dll")] static extern int GetWindowLong(IntPtr window, int index);
    [DllImport("user32.dll")] static extern int SetWindowLong(IntPtr window, int index, int value);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr window);
    [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr window, IntPtr dc);
    [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleDC(IntPtr dc);
    [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr dc);
    [DllImport("gdi32.dll")] static extern IntPtr SelectObject(IntPtr dc, IntPtr obj);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr obj);
    [DllImport("user32.dll")] static extern bool UpdateLayeredWindow(IntPtr window, IntPtr dc, ref POINT dest, ref SIZE size, IntPtr source, ref POINT origin, int key, ref BLEND blend, int flags);
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr window);

    sealed class Overlay : NativeWindow {
        public Overlay(IntPtr parent) {
            CreateParams cp = new CreateParams();
            cp.Caption = "AIHubMix balance";
            cp.Style = unchecked((int)0x80000000); // WS_POPUP until parented
            cp.ExStyle = 0x00080000 | 0x00000020 | 0x08000000; // layered, click-through, no activation
            cp.Width = 1;
            cp.Height = 1;
            CreateHandle(cp);
            SetWindowLong(Handle, -16, (GetWindowLong(Handle, -16) & ~unchecked((int)0x80000000)) | 0x40000000);
            SetParent(Handle, parent);
            if (GetParent(Handle) != parent) {
                DestroyHandle();
                throw new InvalidOperationException("Taskbar parenting failed.");
            }
        }
        public void Dispose() {
            if (Handle != IntPtr.Zero) {
                if (IsWindow(Handle)) DestroyHandle();
                else ReleaseHandle();
            }
        }
    }

    readonly object gate = new object();
    readonly object probeGate = new object();
    readonly System.Windows.Forms.Timer timer = new System.Windows.Forms.Timer();
    string label;
    bool inputClosed;
    Overlay overlay;
    IntPtr taskbar;
    bool probeRunning;
    string probeKey;
    DateTime probeCompleted;
    bool probeBlocked = true;

    public WalletContext() {
        Thread reader = new Thread(ReadInput);
        reader.IsBackground = true;
        reader.Start();
        timer.Interval = 1000;
        timer.Tick += delegate { Tick(); };
        timer.Start();
    }

    void ReadInput() {
        try {
            using (StreamReader input = new StreamReader(Console.OpenStandardInput(), new UTF8Encoding(false), false)) {
                string line;
                while ((line = input.ReadLine()) != null) {
                    lock (gate) {
                        if (line == "CLEAR") label = null;
                        else if (line.StartsWith("VALUE ") && line.Length <= 100)
                            label = line.Substring(6);
                    }
                }
            }
        } catch { }
        lock (gate) inputClosed = true;
        // A hung accessibility provider must never leave a helper behind when
        // its owning Switch process exits.
        Environment.Exit(0);
    }

    void Tick() {
        string value;
        bool closed;
        lock (gate) { value = label; closed = inputClosed; }
        if (closed) {
            timer.Stop();
            if (overlay != null) overlay.Dispose();
            ExitThread();
            return;
        }
        IntPtr task = FindWindow("Shell_TrayWnd", null);
        if (task == IntPtr.Zero || !IsWindowVisible(task)) { Hide(); return; }
        if (taskbar != task || overlay == null || !IsWindow(overlay.Handle)) {
            if (overlay != null) overlay.Dispose();
            taskbar = task;
            try { overlay = new Overlay(task); }
            catch { overlay = null; return; }
        }
        if (String.IsNullOrEmpty(value)) { Hide(); return; }

        RECT taskRect, trayRect;
        IntPtr tray = FindWindowEx(task, IntPtr.Zero, "TrayNotifyWnd", null);
        if (!GetWindowRect(task, out taskRect) || tray == IntPtr.Zero ||
            !GetWindowRect(tray, out trayRect) || !IsWindowVisible(tray) ||
            taskRect.Right - taskRect.Left < (taskRect.Bottom - taskRect.Top) * 3) {
            Hide(); return;
        }
        uint dpi = 96;
        try { dpi = GetDpiForWindow(task); } catch { }
        if (dpi == 0) dpi = 96;
        float scale = dpi / 96f;
        int height = Math.Min(taskRect.Bottom - taskRect.Top - 4, (int)Math.Ceiling(32 * scale));
        if (height < 12) { Hide(); return; }
        bool light = false;
        try {
            object theme = Registry.GetValue(@"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "SystemUsesLightTheme", 0);
            light = Convert.ToInt32(theme) != 0;
        } catch { }
        using (Font font = new Font("Segoe UI", 15 * scale, FontStyle.Regular, GraphicsUnit.Pixel)) {
            int width;
            using (Bitmap probe = new Bitmap(1, 1))
            using (Graphics graphics = Graphics.FromImage(probe))
                width = (int)Math.Ceiling(graphics.MeasureString(value, font).Width + 12 * scale);
            if (width < 1 || width > 240 * scale) { Hide(); return; }
            int x = trayRect.Left - width - (int)Math.Ceiling(8 * scale);
            int y = taskRect.Top + (taskRect.Bottom - taskRect.Top - height) / 2;
            if (x < taskRect.Left || trayRect.Left > taskRect.Right ||
                !HasClearSpace(task, overlay.Handle, x, y, x + width, y + height)) { Hide(); return; }
            Draw(value, font, light, x, y, width, height);
        }
    }

    bool HasClearSpace(IntPtr task, IntPtr ownWindow, int left, int top, int right, int bottom) {
        string key = task.ToInt64() + ":" + left + ":" + top + ":" + right + ":" + bottom;
        lock (probeGate) {
            if (!probeRunning) {
                probeRunning = true;
                ThreadPool.QueueUserWorkItem(delegate {
                    bool blocked = true;
                    try { blocked = Obstructed(task, ownWindow, left, top, right, bottom); }
                    catch { }
                    lock (probeGate) {
                        probeKey = key;
                        probeBlocked = blocked;
                        probeCompleted = DateTime.UtcNow;
                        probeRunning = false;
                    }
                });
            }
            return probeKey == key && !probeBlocked &&
                DateTime.UtcNow - probeCompleted < TimeSpan.FromSeconds(3);
        }
    }

    static bool Obstructed(IntPtr task, IntPtr ownWindow, int left, int top, int right, int bottom) {
        bool foundTaskList = false, blocked = false;
        EnumChildWindows(task, delegate(IntPtr child, IntPtr unused) {
            if (child == ownWindow) return true;
            if (!IsWindowVisible(child)) return true;
            StringBuilder name = new StringBuilder(256);
            GetClassName(child, name, name.Capacity);
            string cls = name.ToString();
            if (cls != "MSTaskSwWClass" && cls != "ReBarWindow32" && cls != "TrayNotifyWnd") return true;
            if (cls == "MSTaskSwWClass") foundTaskList = true;
            RECT r;
            if (GetWindowRect(child, out r) && left < r.Right && right > r.Left && top < r.Bottom && bottom > r.Top)
                blocked = true;
            return true;
        }, IntPtr.Zero);
        // Windows 11 draws some buttons in XAML outside the legacy task-list
        // HWND. Their actual rectangles are exposed through UI Automation.
        if (blocked || !foundTaskList) return true;
        try {
            AutomationElement root = AutomationElement.FromHandle(task);
            Condition controls = new OrCondition(
                new PropertyCondition(AutomationElement.ControlTypeProperty, ControlType.Button),
                new PropertyCondition(AutomationElement.ControlTypeProperty, ControlType.Edit),
                new PropertyCondition(AutomationElement.ControlTypeProperty, ControlType.ListItem));
            AutomationElementCollection elements = root.FindAll(TreeScope.Descendants, controls);
            bool foundVisibleControl = false;
            foreach (AutomationElement element in elements) {
                AutomationElement.AutomationElementInformation current = element.Current;
                if (current.IsOffscreen) continue;
                System.Windows.Rect rect = current.BoundingRectangle;
                if (rect.IsEmpty || rect.Width <= 0 || rect.Height <= 0) continue;
                foundVisibleControl = true;
                if (left < rect.Right && right > rect.Left && top < rect.Bottom && bottom > rect.Top)
                    return true;
            }
            return !foundVisibleControl;
        } catch {
            // When accessibility data is unavailable, retain the native tray
            // icon and tooltip instead of guessing at free space.
            return true;
        }
    }

    void Draw(string value, Font font, bool light, int x, int y, int width, int height) {
        using (Bitmap bitmap = new Bitmap(width, height, PixelFormat.Format32bppPArgb))
        using (Graphics graphics = Graphics.FromImage(bitmap))
        using (Brush brush = new SolidBrush(light ? Color.FromArgb(28, 30, 33) : Color.White))
        using (StringFormat format = new StringFormat()) {
            graphics.Clear(Color.Transparent);
            graphics.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
            format.Alignment = StringAlignment.Center;
            format.LineAlignment = StringAlignment.Center;
            graphics.DrawString(value, font, brush, new RectangleF(0, 0, width, height), format);
            IntPtr screen = GetDC(IntPtr.Zero);
            IntPtr memory = CreateCompatibleDC(screen);
            IntPtr image = bitmap.GetHbitmap(Color.FromArgb(0));
            IntPtr old = SelectObject(memory, image);
            try {
                POINT dest = new POINT(x, y), origin = new POINT(0, 0);
                SIZE size = new SIZE(width, height);
                BLEND blend = new BLEND { Op = 0, Flags = 0, Alpha = 255, Format = 1 };
                if (!UpdateLayeredWindow(overlay.Handle, screen, ref dest, ref size, memory, ref origin, 0, ref blend, 2)) { Hide(); return; }
            } finally {
                SelectObject(memory, old); DeleteObject(image); DeleteDC(memory); ReleaseDC(IntPtr.Zero, screen);
            }
            RECT taskRect;
            if (!GetWindowRect(taskbar, out taskRect)) { Hide(); return; }
            // Coordinates for SetWindowPos on a child are relative to its parent.
            SetWindowPos(overlay.Handle, IntPtr.Zero, x - taskRect.Left, y - taskRect.Top, width, height, 0x0010 | 0x0040);
        }
    }

    void Hide() {
        if (overlay != null && IsWindow(overlay.Handle)) ShowWindow(overlay.Handle, 0);
    }

    [STAThread] static void Main() {
        Application.EnableVisualStyles();
        Application.Run(new WalletContext());
    }
}
