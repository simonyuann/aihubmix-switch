// T22 收尾子批 locale keys:codexDesktop(10)/announcements(11)/accountPage(+11)/imageModelSelect(+2)
import fs from "node:fs";

const codexDesktop = {
  launch: {
    restartAfterQuit: {
      zh: "请保存工作并完全退出 Codex（包括托盘），再重新启动。",
      en: "Save your work and fully quit Codex (including the tray), then relaunch.",
      ja: "作業内容を保存し、Codex（トレイ含む）を完全に終了してから再起動してください。",
      tr: "Çalışmanızı kaydedin ve Codex'i (sistem tepsisi dahil) tamamen kapatın, sonra yeniden başlatın.",
      tw: "請儲存工作並完全退出 Codex（包括系統匣），再重新啟動。",
    },
    manualQuitRetry: {
      zh: "未能自动退出 Codex，请保存工作并手动退出后重试。",
      en: "Codex could not quit automatically. Save your work, quit manually, and retry.",
      ja: "Codex を自動終了できませんでした。作業を保存して手動で終了し、再試行してください。",
      tr: "Codex otomatik olarak kapatılamadı. Çalışmanızı kaydedin, elle kapatın ve yeniden deneyin.",
      tw: "未能自動退出 Codex，請儲存工作並手動退出後重試。",
    },
    notFoundTrusted: {
      zh: "未找到受信任的 Codex Desktop，请在设置中选择程序路径。",
      en: "No trusted Codex Desktop found. Choose the program path in Settings.",
      ja: "信頼できる Codex Desktop が見つかりません。設定でプログラムパスを選択してください。",
      tr: "Güvenilir bir Codex Desktop bulunamadı. Ayarlar'dan program yolunu seçin.",
      tw: "未找到受信任的 Codex Desktop，請在設定中選擇程式路徑。",
    },
    unsupported: {
      zh: "当前系统或 Codex 版本暂不支持从这里启动。",
      en: "Launching from here is not supported on this system or Codex version yet.",
      ja: "現在のシステムまたは Codex バージョンでは、ここからの起動は未対応です。",
      tr: "Bu sistem veya Codex sürümü buradan başlatmayı henüz desteklemiyor.",
      tw: "目前系統或 Codex 版本暫不支援從這裡啟動。",
    },
    localeUnconfirmed: {
      zh: "界面语言未能确认，请在 Codex 中检查。",
      en: "Could not confirm the UI language. Please check inside Codex.",
      ja: "UI 言語を確認できませんでした。Codex 内でご確認ください。",
      tr: "Arayüz dili doğrulanamadı. Lütfen Codex içinde kontrol edin.",
      tw: "介面語言未能確認，請在 Codex 中檢查。",
    },
    failedFallback: {
      zh: "启动失败，请检查 Codex Desktop 是否已安装后重试。",
      en: "Launch failed. Check that Codex Desktop is installed and try again.",
      ja: "起動に失敗しました。Codex Desktop がインストール済みか確認して再試行してください。",
      tr: "Başlatma başarısız oldu. Codex Desktop'ın yüklü olduğunu kontrol edip yeniden deneyin.",
      tw: "啟動失敗，請檢查 Codex Desktop 是否已安裝後重試。",
    },
  },
  idle: {
    notInstalled: {
      zh: "未找到 Codex，请在设置中选择程序。",
      en: "Codex not found. Choose the program in Settings.",
      ja: "Codex が見つかりません。設定でプログラムを選択してください。",
      tr: "Codex bulunamadı. Ayarlar'dan programı seçin.",
      tw: "未找到 Codex，請在設定中選擇程式。",
    },
  },
  action: {
    launch: { zh: "启动 Codex", en: "Launch Codex", ja: "Codex を起動", tr: "Codex'i başlat", tw: "啟動 Codex" },
    launching: { zh: "正在启动", en: "Launching", ja: "起動中", tr: "Başlatılıyor", tw: "正在啟動" },
    managePlugins: { zh: "管理插件", en: "Manage plugins", ja: "プラグインを管理", tr: "Eklentileri yönet", tw: "管理外掛" },
  },
};

const announcements = {
  banner: {
    new: { zh: "新公告", en: "New announcements", ja: "新しいお知らせ", tr: "Yeni duyurular", tw: "新公告" },
    unreadCount: {
      zh: "{{count}} 条未读",
      en: "{{count}} unread",
      ja: "未読 {{count}} 件",
      tr: "{{count}} okunmamış",
      tw: "{{count}} 則未讀",
    },
  },
  error: {
    badPayload: {
      zh: "公告数据格式异常",
      en: "Announcement data has an unexpected format",
      ja: "お知らせデータの形式が不正です",
      tr: "Duyuru verisi biçimi geçersiz",
      tw: "公告資料格式異常",
    },
    loadFailed: {
      zh: "公告加载失败，请重试。",
      en: "Failed to load announcements. Please retry.",
      ja: "お知らせの読み込みに失敗しました。再試行してください。",
      tr: "Duyurular yüklenemedi. Lütfen yeniden deneyin.",
      tw: "公告載入失敗，請重試。",
    },
    saveStateFailed: {
      zh: "公告状态保存失败，请重试。",
      en: "Failed to save announcement state. Please retry.",
      ja: "お知らせの状態保存に失敗しました。再試行してください。",
      tr: "Duyuru durumu kaydedilemedi. Lütfen yeniden deneyin.",
      tw: "公告狀態儲存失敗，請重試。",
    },
  },
  title: { zh: "公告", en: "Announcements", ja: "お知らせ", tr: "Duyurular", tw: "公告" },
  action: {
    backToList: {
      zh: "返回公告列表",
      en: "Back to announcement list",
      ja: "お知らせ一覧に戻る",
      tr: "Duyuru listesine dön",
      tw: "返回公告列表",
    },
    refresh: {
      zh: "刷新公告",
      en: "Refresh announcements",
      ja: "お知らせを更新",
      tr: "Duyuruları yenile",
      tw: "重新整理公告",
    },
  },
  aria: {
    unread: { zh: "（未读）", en: "(unread)", ja: "（未読）", tr: "(okunmamış)", tw: "（未讀）" },
  },
  status: {
    loading: {
      zh: "正在加载公告…",
      en: "Loading announcements…",
      ja: "お知らせを読み込み中…",
      tr: "Duyurular yükleniyor…",
      tw: "正在載入公告…",
    },
  },
  empty: {
    none: { zh: "暂无公告", en: "No announcements yet", ja: "お知らせはありません", tr: "Henüz duyuru yok", tw: "暫無公告" },
  },
};

const accountPage = {
  notice: {
    keyDeleted: {
      zh: "已删除 API Key「{{name}}」",
      en: 'API Key "{{name}}" deleted',
      ja: "API Key「{{name}}」を削除しました",
      tr: '"{{name}}" API anahtarı silindi',
      tw: "已刪除 API Key「{{name}}」",
    },
  },
  error: {
    deleteUnconfirmedDetail: {
      zh: "{{detail}}。请刷新列表确认删除结果后再重试。",
      en: "{{detail}}. Refresh the list to confirm the result, then retry.",
      ja: "{{detail}}。一覧を更新して結果を確認してから、再試行してください。",
      tr: "{{detail}}. Sonucu doğrulamak için listeyi yenileyin, sonra tekrar deneyin.",
      tw: "{{detail}}。請重新整理列表確認刪除結果後再重試。",
    },
  },
  label: {
    currentAccount: {
      zh: "当前账号",
      en: "Current account",
      ja: "現在のアカウント",
      tr: "Geçerli hesap",
      tw: "目前帳號",
    },
  },
  section: {
    myKeys: { zh: "我的 API Keys", en: "My API Keys", ja: "マイ API Keys", tr: "API anahtarlarım", tw: "我的 API Keys" },
  },
  action: {
    refresh: { zh: "刷新", en: "Refresh", ja: "更新", tr: "Yenile", tw: "重新整理" },
    copyKey: { zh: "复制 Key", en: "Copy Key", ja: "Key をコピー", tr: "Anahtarı kopyala", tw: "複製 Key" },
    deleteKey: { zh: "删除", en: "Delete", ja: "削除", tr: "Sil", tw: "刪除" },
  },
  aria: {
    copyKey: { zh: "复制 {{name}}", en: "Copy {{name}}", ja: "{{name}} をコピー", tr: "{{name}} kopyala", tw: "複製 {{name}}" },
    deleteKey: { zh: "删除 {{name}}", en: "Delete {{name}}", ja: "{{name}} を削除", tr: "{{name}} sil", tw: "刪除 {{name}}" },
  },
  empty: {
    bindHint: {
      zh: "在首页选择工具并添加模型分组，AIHub 会自动绑定可用的 API Key。模型可在 Agent 中通过 /model 切换。",
      en: "Pick a tool and add a model group on the home page; AIHub will bind an available API Key automatically. Switch models with /model in the Agent.",
      ja: "ホームでツールを選びモデルグループを追加すると、AIHub が利用可能な API Key を自動的に紐付けます。モデルは Agent 内で /model で切り替えられます。",
      tr: "Ana sayfada bir araç seçip model grubu ekleyin; AIHub kullanılabilir bir API anahtarını otomatik bağlar. Modelleri Agent içinde /model ile değiştirin.",
      tw: "在首頁選擇工具並新增模型分組，AIHub 會自動綁定可用的 API Key。模型可在 Agent 中透過 /model 切換。",
    },
  },
  dialog: {
    deleteBody: {
      zh: "确定删除「{{name}}」？删除后，使用此 Key 的工具将无法继续调用 API，此操作无法撤销。",
      en: 'Delete "{{name}}"? Tools using this Key will no longer be able to call the API. This cannot be undone.',
      ja: "「{{name}}」を削除しますか？この Key を使うツールは API を呼び出せなくなります。取り消せません。",
      tr: '"{{name}}" silinsin mi? Bu anahtarı kullanan araçlar API çağıramaz. Bu işlem geri alınamaz.',
      tw: "確定刪除「{{name}}」？刪除後，使用此 Key 的工具將無法繼續呼叫 API，此操作無法撤銷。",
    },
  },
};

const imageModelSelect = {
  hint: {
    keepRunning: {
      zh: "生图和改图使用此型号。请保持 AIHub 运行。",
      en: "Image generation and editing use this model. Keep AIHub running.",
      ja: "画像生成・編集はこのモデルを使用します。AIHub を起動したままにしてください。",
      tr: "Görsel üretim ve düzenleme bu modeli kullanır. AIHub'ı çalışır durumda tutun.",
      tw: "生圖和改圖使用此型號。請保持 AIHub 運行。",
    },
    gatewayOnly: {
      zh: "当前分组仅提供网关默认选项。",
      en: "This group only offers the gateway default.",
      ja: "このグループはゲートウェイのデフォルトのみを提供します。",
      tr: "Bu grup yalnızca ağ geçidi varsayılanını sunar.",
      tw: "目前分組僅提供閘道預設選項。",
    },
  },
};

// 把 {zh,en,...} → 目标 locale 值
function localize(node, locale) {
  const field = locale === "zh-TW" ? "tw" : locale;
  if (node && typeof node === "object" && "zh" in node && "en" in node) {
    if (!(field in node)) throw new Error(`missing locale field ${field}`);
    return node[field];
  }
  const out = {};
  for (const [k, v] of Object.entries(node)) out[k] = localize(v, locale);
  return out;
}

const additions = { codexDesktop, announcements, accountPage, imageModelSelect };
const files = {
  zh: "src/i18n/locales/zh.json",
  en: "src/i18n/locales/en.json",
  ja: "src/i18n/locales/ja.json",
  tr: "src/i18n/locales/tr.json",
  "zh-TW": "src/i18n/locales/zh-TW.json",
};

for (const [locale, file] of Object.entries(files)) {
  const data = JSON.parse(fs.readFileSync(file, "utf8"));
  for (const [ns, body] of Object.entries(additions)) {
    const translated = localize(body, locale);
    if (data[ns]) {
      // 深合并进既有 namespace(accountPage/imageModelSelect)
      const merge = (dst, src) => {
        for (const [k, v] of Object.entries(src)) {
          if (dst[k] && typeof dst[k] === "object" && typeof v === "object" && !Array.isArray(v)) merge(dst[k], v);
          else dst[k] = v;
        }
      };
      merge(data[ns], translated);
    } else {
      data[ns] = translated;
    }
  }
  fs.writeFileSync(file, JSON.stringify(data, null, 2) + "\n");
  console.log(`${locale}: keys added`);
}
console.log("locale additions done");
