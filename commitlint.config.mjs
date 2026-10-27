export default {
	extends: ["@commitlint/config-conventional"],
	rules: {
		// Dependabot の本文は 1 行 100 文字を超える
		"body-max-line-length": [0],
		"footer-max-line-length": [0],
		// 日本語の説明には英語の大文字小文字の規則が当てはまらない
		"subject-case": [0],
	},
};
