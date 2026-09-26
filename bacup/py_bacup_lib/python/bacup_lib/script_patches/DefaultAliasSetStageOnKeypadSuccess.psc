; Receives the keypad's success event and advances the owning quest exactly
; once. It deliberately does not implement OnActivate: activation is not a
; result, and treating it as one is the defect this replaces.

Event OnAliasInit()
    RegisterKeypad()
EndEvent

Event OnAliasShutdown()
    ; FO4 clears the alias before this event; retain the registered reference.
    If registeredKeypad != None
        UnregisterForCustomEvent(registeredKeypad, "KeypadSuccess")
        registeredKeypad.UnregisterCompletionDelegate(Self)
        registeredKeypad = None
    EndIf
EndEvent

Event OnLoad()
    RegisterKeypad()
EndEvent

Function RegisterKeypad()
    DefaultKeypadScript keypad = GetKeypad()
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
    If akArgs == None || akArgs.Length < 2
        Return
    EndIf
    ApplyKeypadSuccess(akSender, akArgs[1] as ObjectReference)
EndEvent

DefaultKeypadScript Function GetKeypad()
    ObjectReference aliasRef = GetReference()
    If aliasRef == None
        Return None
    EndIf
    Return aliasRef as DefaultKeypadScript
EndFunction

Function ApplyKeypadSuccess(DefaultKeypadScript akKeypad, ObjectReference akActivator)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning() || StageToSet < 0
        Return
    EndIf
    If preReqStage > 0 && !owningQuest.IsStageDone(preReqStage)
        Return
    EndIf
    If !ActivatorIsAllowed(akActivator)
        Return
    EndIf
    If TurnOffStage >= 0 && owningQuest.GetStage() >= TurnOffStage
        Return
    EndIf
    If !owningQuest.IsStageDone(StageToSet)
        If owningQuest.SetStage(StageToSet)
            akKeypad.CompleteKeypad()
        EndIf
    EndIf
EndFunction

; PlayerActivateType 0 is FO76's ANY; 1/2/3 all narrow to the active player,
; which single-player Fallout 4 collapses to Game.GetPlayer().
Bool Function ActivatorIsAllowed(ObjectReference akActivator)
    If akActivator == None
        Return False
    EndIf
    If PlayerActivateType != 0 && akActivator != Game.GetPlayer()
        Return False
    EndIf
    If ActivatedByReferences != None && ActivatedByReferences.Length > 0
        If ActivatedByReferences.Find(akActivator) < 0
            Return False
        EndIf
    EndIf
    If ActivatedByAliases != None && ActivatedByAliases.Length > 0
        If !AliasesHold(akActivator)
            Return False
        EndIf
    EndIf
    If ActivatedByFactions != None && ActivatedByFactions.Length > 0
        If !FactionsHold(akActivator)
            Return False
        EndIf
    EndIf
    Return True
EndFunction

Bool Function AliasesHold(ObjectReference akActivator)
    Int index = 0
    While index < ActivatedByAliases.Length
        ReferenceAlias candidate = ActivatedByAliases[index]
        If candidate != None && candidate.GetReference() == akActivator
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Bool Function FactionsHold(ObjectReference akActivator)
    Actor activatingActor = akActivator as Actor
    If activatingActor == None
        Return False
    EndIf
    Int index = 0
    While index < ActivatedByFactions.Length
        Faction candidate = ActivatedByFactions[index]
        If candidate != None && activatingActor.IsInFaction(candidate)
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction
