const base = require("./base");

module.exports = {
  ...base,
  config: someDynamicFunction(),
};
