Scriptname B21:KeypadNative Native Hidden

Int Function ReadCode(ObjectReference keypad, Int digits) Global Native
Bool Function Ready() Global Native
Bool Function ValidateLaunchCode(Int seed, Int week, Int silo, Int code) Global Native
String Function PieceText(Int seed, Int week, Int silo, Int piece) Global Native
String Function Briefing(Int seed, Int week, Int silo, Float days) Global Native
String[] Function KeywordLetters(Int seed, Int week, Int silo, Float days) Global Native
Function RenamePiece(Form pieceForm, Int seed, Int week, Int silo, Int piece) Global Native
