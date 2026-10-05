console.log("CONFIG FILE LOADED");

window.MathJax = {
  loader: {
    load: ['[tex]/boldsymbol', '[tex]/ams']  // ← no bm here
  },
  tex: {
    inlineMath: [["$", "$"], ["\\(", "\\)"]],
    displayMath: [["$$", "$$"], ["\\[", "\\]"]],
    packages: { '[+]': ['boldsymbol', 'ams'] },  // ← no bm here
    macros: {
      mathbf: ["{\\boldsymbol{#1}}", 1]
    }
  },
  svg: {
    fontCache: "global"
  }
};

window.renderMath = () => {
  if (!window.MathJax) return;
  if (!MathJax.typesetPromise) return;

  MathJax.typesetClear();        // clear previous typesetting
  MathJax.typesetPromise();      // re-typeset current DOM
};
