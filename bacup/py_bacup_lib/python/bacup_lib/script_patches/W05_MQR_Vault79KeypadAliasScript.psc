Event OnAliasInit()
    RegisterKeypad()
EndEvent

Event OnLoad()
    RegisterKeypad()
EndEvent

Event OnAliasShutdown()
    If registeredKeypad != None
        UnregisterForCustomEvent(registeredKeypad, "KeypadSuccess")
        registeredKeypad.UnregisterCompletionDelegate(Self)
        registeredKeypad = None
    EndIf
EndEvent

Function RegisterKeypad()
    ObjectReference keypadRef = GetReference()
    DefaultKeypadScript keypad = keypadRef as DefaultKeypadScript
    If registeredKeypad != None && registeredKeypad != keypad
        UnregisterForCustomEvent(registeredKeypad, "KeypadSuccess")
        registeredKeypad.UnregisterCompletionDelegate(Self)
    EndIf
    registeredKeypad = keypad
    If keypad != None
        RegisterForCustomEvent(keypad, "KeypadSuccess")
        keypad.RegisterCompletionDelegate(Self)
    EndIf
EndFunction

Event DefaultKeypadScript.KeypadSuccess(DefaultKeypadScript akSender, Var[] akArgs)
    If akSender == None || akSender != registeredKeypad || (akSender as ObjectReference) != GetReference()
        Return
    EndIf
    If akArgs == None || akArgs.Length < 2 || (akArgs[1] as ObjectReference) != Game.GetPlayer()
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning() || StageToSet < 0
        Return
    EndIf
    If PreReqStage >= 0 && !owningQuest.IsStageDone(PreReqStage)
        Return
    EndIf
    If !owningQuest.IsStageDone(StageToSet)
        If owningQuest.SetStage(StageToSet)
            akSender.CompleteKeypad()
        EndIf
    EndIf
EndEvent
