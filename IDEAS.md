# Ideas

## Features

### observability / logging
* report wifi signal strength on some interval
* report cpu temperature if there is one
* ability to set and change logging level
* probably need to make the logs less chatty: which message really matter?

### widgets
* can we support a .webp image display? animated?
  * this would be the first step towards RGB displays
  * initially only support monocrome .webp images or convert to mono on the fly... transparent and black are not lit and everything else is lit.
  * how much harder is animated .webp?
  * possibly remove the raw pixel widget type for matrix displays and use this instead
  * would not work on tm1637
