Int Function DigitCommitTimerID() Global
    Return 76001
EndFunction

Int Function DigitCount()
    If presetCode <= 0 && KeypadCode == None && DynamicCode() >= 0
        Return 6
    EndIf
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

Function RegisterCompletionDelegate(ReferenceAlias akDelegate)
    If akDelegate == None
        Return
    EndIf
    If completionDelegates == None
        completionDelegates = new ReferenceAlias[0]
    EndIf
    If completionDelegates.Find(akDelegate) < 0
        completionDelegates.Add(akDelegate)
        bKeypadSolved = False
        ResetEntry()
    EndIf
EndFunction

Function UnregisterCompletionDelegate(ReferenceAlias akDelegate)
    If completionDelegates != None
        Int index = completionDelegates.Find(akDelegate)
        If index >= 0
            completionDelegates.Remove(index)
        EndIf
    EndIf
EndFunction

Bool Function HasCompletionDelegate()
    Int index = 0
    While completionDelegates != None && index < completionDelegates.Length
        ReferenceAlias listener = completionDelegates[index]
        If listener != None && listener.GetReference() == Self
            Quest owningQuest = listener.GetOwningQuest()
            If owningQuest != None && owningQuest.IsRunning()
                Return True
            EndIf
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Function ResetEntry()
    CancelTimer(DigitCommitTimerID())
    iEntryGeneration += 1
    bDigitInProgress = False
    iEntryDigit = 0
    iEntryPosition = 0
    iEnteredCode = 0
EndFunction

Event OnActivate(ObjectReference akActionRef)
    If bKeypadSolved && !HasCompletionDelegate()
        Return
    EndIf
    If akActionRef == None || akActionRef != Game.GetPlayer()
        Return
    EndIf
    If ExpectedCode() < 0 || bDigitInProgress
        Return
    EndIf
    If !B21:KeypadNative.Ready()
        Debug.Notification("The Tales keypad runtime is unavailable.")
        Return
    EndIf
    ResetEntry()
    Int entryGeneration = iEntryGeneration
    bDigitInProgress = True
    Int enteredCode = B21:KeypadNative.ReadCode(Self, DigitCount())
    If entryGeneration != iEntryGeneration
        Return
    EndIf
    bDigitInProgress = False
    If enteredCode < 0 || (bKeypadSolved && !HasCompletionDelegate())
        Return
    EndIf
    iEnteredCode = enteredCode
    EvaluateEnteredCode()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == DigitCommitTimerID()
        ResetEntry()
    EndIf
EndEvent

Event OnUnload()
    If !bKeypadSolved
        ResetEntry()
    EndIf
EndEvent

Event OnLoad()
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    ResetEntry()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        ResetEntry()
    EndIf
EndEvent

Function EvaluateEnteredCode()
    Int expectedCode = ExpectedCode()
    Int enteredCode = iEnteredCode
    ResetEntry()
    If expectedCode < 0 || enteredCode != expectedCode
        Debug.Notification("The keypad rejects that code.")
        Return
    EndIf
    Bool delegated = HasCompletionDelegate()
    Var[] successArgs = new Var[2]
    successArgs[0] = enteredCode
    successArgs[1] = Game.GetPlayer()
    SendCustomEvent("KeypadSuccess", successArgs)
    If !delegated
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
