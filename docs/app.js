document.querySelectorAll('[data-tabs]').forEach((group) => {
  const buttons = group.querySelectorAll('[role="tab"]');
  const panels = group.querySelectorAll('[role="tabpanel"]');
  buttons.forEach((button) => button.addEventListener('click', () => {
    const target = `tab-${button.dataset.tab}`;
    buttons.forEach((item) => item.setAttribute('aria-selected', String(item === button)));
    panels.forEach((panel) => {
      const active = panel.id === target;
      panel.hidden = !active;
      panel.classList.toggle('is-active', active);
    });
  }));
});

document.querySelectorAll('[data-copy]').forEach((button) => button.addEventListener('click', async () => {
  const source = document.querySelector(button.dataset.copy);
  if (!source) return;
  await navigator.clipboard.writeText(source.textContent.trim());
  const original = button.textContent;
  button.textContent = 'Copied';
  setTimeout(() => { button.textContent = original; }, 1600);
}));
