; @extends Terminal
; FO76 delivered OnMenuItemRun to this ObjectReference script on the TERM base; in FO4
; only a Terminal script receives it, so the script moves to Terminal and works on
; akTerminalRef. The entry shows the pending code through its GeneratedKeyDigit
; tokens, so the code on screen is committed to the keypad on that run and the next
; one is prepared at once: reopening the entry shows a new key and invalidates the
; old one. The very first run has no prepared code yet and shows blank digits.
Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    If auiMenuItemID != RunOnMenuID || akTerminalRef == None
        Return
    EndIf
    If B21PendingCode <= 0
        PrepareNextCode(akTerminalRef)
    EndIf
    CommitCode(akTerminalRef, B21PendingCode)
    PrepareNextCode(akTerminalRef)
EndEvent

Function PrepareNextCode(ObjectReference akTerminalRef)
    Int digits = codeLength
    If digits < 1
        digits = 1
    ElseIf digits > 9
        digits = 9
    EndIf
    ObjectReference textTarget = akTerminalRef
    If !SetTextReplacementOnSelf && KeypadTextReplacementKeyword != None
        textTarget = akTerminalRef.GetLinkedRef(KeypadTextReplacementKeyword)
    EndIf
    Int code = 0
    Int position = 1
    While position <= 9
        Form digitForm = None
        If position <= digits
            Int digit = Utility.RandomInt(0, 9)
            If position == 1 && digit == 0
                digit = Utility.RandomInt(1, 9)
            EndIf
            code = code * 10 + digit
            If KeypadDigitsList != None
                digitForm = KeypadDigitsList.GetAt(digit)
            EndIf
        ElseIf KeypadDigitsList != None && KeypadDigitsList.GetSize() > 10
            digitForm = KeypadDigitsList.GetAt(10)
        EndIf
        If textTarget != None && digitForm != None
            textTarget.AddTextReplacementData("GeneratedKeyDigit" + position, digitForm)
        EndIf
        position += 1
    EndWhile
    B21PendingCode = code
EndFunction

Function CommitCode(ObjectReference akTerminalRef, Int aiCode)
    ObjectReference keypadRef = akTerminalRef
    If !SetKeypadCodeOnSelf
        keypadRef = None
        If KeypadLinkKeyword != None
            keypadRef = akTerminalRef.GetLinkedRef(KeypadLinkKeyword)
        EndIf
    EndIf
    If keypadRef == None || aiCode <= 0
        Return
    EndIf
    DefaultKeypadScript defaultKeypad = keypadRef as DefaultKeypadScript
    If defaultKeypad != None
        defaultKeypad.codeNumDigits = codeLength
        If defaultKeypad.KeypadCode != None
            keypadRef.SetValue(defaultKeypad.KeypadCode, aiCode as Float)
        EndIf
        Return
    EndIf
    LinkKeypadToUnlock linkedKeypad = keypadRef as LinkKeypadToUnlock
    If linkedKeypad != None
        linkedKeypad.SetCode(aiCode, codeLength)
    EndIf
EndFunction
