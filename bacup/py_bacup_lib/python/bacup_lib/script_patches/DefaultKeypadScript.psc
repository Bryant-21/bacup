; Fallout 76 opened a native numeric-entry menu from the ShowKeypadOnActivate
; keyword and reported the result through a native success event. Fallout 4 has
; neither, so entry is rebuilt from the two things the base game does have: the
; Vitale Pumphouse puzzle's per-digit selection, and MS07c's keypad contract
; where activation alone never advances anything.
;
; Activating cycles the digit under the cursor; pausing commits it. Only a
; complete code equal to the keypad's own code counts as success, and only the
; owner of the outcome performs the effect: a quest alias that has taken the
; completion over can refuse it after its own prerequisite and actor checks, and
; the keypad then stays open for another attempt.

Float Function DigitCommitSeconds() Global
    Return 1.5
EndFunction

Int Function DigitCommitTimerID() Global
    Return 76001
EndFunction

Int Function DigitCount()
    If codeNumDigits > 0
        Return codeNumDigits
    EndIf
    Return 4
EndFunction

Int Function ExpectedCode()
    If presetCode > 0
        Return presetCode
    EndIf
    If KeypadCode != None
        Float storedCode = GetValue(KeypadCode)
        If storedCode >= 0.0
            Return storedCode as Int
        EndIf
    EndIf
    Return DynamicCode()
EndFunction

; Vault 79's keypad reference binds this script with no properties at all, so
; neither presetCode nor KeypadCode can carry its code: the code is generated
; per player and lives in W05_MQ00_CodeAV, which the sibling script on the same
; reference does bind. Reading it through that sibling keeps the FormID out of a
; script 26 keypads share and scopes the fallback to the references that carry
; it. Still fail-closed while the value is its -1 default.
Int Function DynamicCode()
    ObjectReference keypadRef = Self as ObjectReference
    W05_Vaut79EntranceKeypadScript vaultKeypad = keypadRef as W05_Vaut79EntranceKeypadScript
    If vaultKeypad == None || vaultKeypad.W05_MQ00_CodeAV == None
        Return -1
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return -1
    EndIf
    Float dynamicCode = playerRef.GetValue(vaultKeypad.W05_MQ00_CodeAV)
    If dynamicCode < 0.0
        Return -1
    EndIf
    Return dynamicCode as Int
EndFunction

Function SetCompletionDelegated(Bool abDelegated)
    bCompletionDelegated = abDelegated
EndFunction

Function ResetEntry()
    CancelTimer(DigitCommitTimerID())
    bDigitInProgress = False
    iEntryDigit = 0
    iEntryPosition = 0
    iEnteredCode = 0
EndFunction

Event OnActivate(ObjectReference akActionRef)
    If bKeypadSolved
        Return
    EndIf
    If akActionRef == None || akActionRef != Game.GetPlayer()
        Return
    EndIf
    If ExpectedCode() < 0
        Return
    EndIf
    If bDigitInProgress
        iEntryDigit = iEntryDigit + 1
        If iEntryDigit > 9
            iEntryDigit = 0
        EndIf
    Else
        bDigitInProgress = True
        iEntryDigit = 0
    EndIf
    Debug.Notification("Keypad digit " + (iEntryPosition + 1) + " of " + DigitCount() + ": " + iEntryDigit)
    StartTimer(DigitCommitSeconds(), DigitCommitTimerID())
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == DigitCommitTimerID()
        CommitDigit()
    EndIf
EndEvent

Event OnUnload()
    If !bKeypadSolved
        ResetEntry()
    EndIf
EndEvent

Function CommitDigit()
    If bKeypadSolved || !bDigitInProgress
        Return
    EndIf
    iEnteredCode = iEnteredCode * 10 + iEntryDigit
    iEntryPosition = iEntryPosition + 1
    bDigitInProgress = False
    If iEntryPosition < DigitCount()
        Return
    EndIf
    EvaluateEnteredCode()
EndFunction

Function EvaluateEnteredCode()
    Int expectedCode = ExpectedCode()
    Int enteredCode = iEnteredCode
    ResetEntry()
    If expectedCode < 0 || enteredCode != expectedCode
        Debug.Notification("The keypad rejects that code.")
        Return
    EndIf
    Var[] successArgs = new Var[1]
    successArgs[0] = enteredCode
    SendCustomEvent("KeypadSuccess", successArgs)
    If !bCompletionDelegated
        CompleteKeypad()
    EndIf
EndFunction

; Idempotent: the delegate calls this once its own guards pass, and a keypad
; nothing is listening to calls it directly.
Function CompleteKeypad()
    If bKeypadSolved
        Return
    EndIf
    bKeypadSolved = True
    ResetEntry()
    UnlockLinkedReference()
    Debug.Notification("The keypad accepts the code.")
EndFunction

Function UnlockLinkedReference()
    ObjectReference linkedRef = None
    If LinkKeyword != None
        linkedRef = GetLinkedRef(LinkKeyword)
    EndIf
    If linkedRef == None
        linkedRef = GetLinkedRef()
    EndIf
    If linkedRef == None
        Return
    EndIf
    linkedRef.Unlock()
    linkedRef.SetOpen(True)
EndFunction
