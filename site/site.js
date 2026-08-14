const languageButton = document.querySelector('.language');
const translatable = document.querySelectorAll('[data-en][data-zh]');
const mediaAlt = {
  en: 'Synthetic webClx demonstration showing a desktop coding terminal continued from a phone',
  zh: 'webClx 合成演示：在电脑编程终端开始任务并从手机继续',
};

function setLanguage(language) {
  document.documentElement.lang = language === 'zh' ? 'zh-CN' : 'en';
  translatable.forEach((element) => {
    element.textContent = element.dataset[language];
  });
  document.querySelector('.hero-media img').alt = mediaAlt[language];
  languageButton.textContent = language === 'zh' ? 'English' : '中文';
  languageButton.setAttribute('aria-pressed', language === 'zh' ? 'true' : 'false');
  localStorage.setItem('webclx-site-language', language);
}

languageButton.addEventListener('click', () => {
  setLanguage(document.documentElement.lang.startsWith('zh') ? 'en' : 'zh');
});

const preferred = localStorage.getItem('webclx-site-language')
  || (navigator.language.startsWith('zh') ? 'zh' : 'en');
setLanguage(preferred);
