Function RegisterInteractionEvents()
    ObjectReference player = Alias_Player.GetReference()
    If player != None
        RegisterForRemoteEvent(player, "OnPlayerLoadGame")
    EndIf
    ObjectReference backDoor = BackDoorActivator.GetReference()
    If backDoor != None
        RegisterForRemoteEvent(backDoor, "OnActivate")
    EndIf
    Int index = 0
    While index < ValveActivationList.Length
        ObjectReference valve = ValveActivationList[index].GetReference()
        If valve != None
            RegisterForRemoteEvent(valve, "OnActivate")
        EndIf
        index += 1
    EndWhile
    index = 0
    While index < Alias_Actors_LostMunis.GetCount()
        ObjectReference muni = Alias_Actors_LostMunis.GetAt(index)
        If muni != None
            RegisterForRemoteEvent(muni, "OnActivate")
        EndIf
        index += 1
    EndWhile
EndFunction

Function RebuildMuniProgress()
    MunisHelpedCount = 0
    If !IsStageDone(700)
        Return
    EndIf
    Int index = 0
    While index < Alias_Actors_LostMunis.GetCount()
        ObjectReference muni = Alias_Actors_LostMunis.GetAt(index)
        If muni != None && !muni.HasKeyword(AC_MQ03_HonorBound_MuniDowned_Keyword)
            MunisHelpedCount += 1
        EndIf
        index += 1
    EndWhile
EndFunction

Function BeginMuniRescue()
    RebuildMuniProgress()
    RegisterInteractionEvents()
EndFunction

Function BeginBackDoorPuzzle()
    DoorActivationCount = 0
    RegisterInteractionEvents()
EndFunction

Function BeginValvePuzzle()
    ValvesActivatedCounter = 0
    RegisterInteractionEvents()
EndFunction

Bool Function IsValve(ObjectReference candidate)
    Int index = 0
    While index < ValveActivationList.Length
        If ValveActivationList[index].GetReference() == candidate
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Function HandleValveActivation(ObjectReference valve)
    If !IsStageDone(1500) || IsStageDone(ChemLabFloodedStage)
        Return
    EndIf
    If ValvesActivatedCounter < ValveActivationList.Length && ValveActivationList[ValvesActivatedCounter].GetReference() == valve
        ValvesActivatedCounter += 1
        If ValvesActivatedCounter >= ValveActivationList.Length
            SetStage(ChemLabFloodedStage)
        EndIf
        Return
    EndIf
    ValvesActivatedCounter = 0
    If MessedUpValvesScene != None && !MessedUpValvesScene.IsPlaying()
        MessedUpValvesScene.Start()
    EndIf
EndFunction

Function HandleBackDoorActivation()
    If !IsStageDone(1000) || IsStageDone(OpenedBackDoorStage)
        Return
    EndIf
    DoorActivationCount += 1
    If DoorActivationCount >= NumTimesToActivateDoor
        ObjectReference backDoor = BackDoorActivator.GetReference()
        If backDoor != None
            backDoor.BlockActivation(False)
            backDoor.Lock(False)
        EndIf
        SetStage(OpenedBackDoorStage)
    EndIf
EndFunction

Function HelpMuni(ObjectReference muniRef)
    If !IsStageDone(700) || IsStageDone(MunisHelpedStage) || muniRef == None || !muniRef.HasKeyword(AC_MQ03_HonorBound_MuniDowned_Keyword)
        Return
    EndIf
    muniRef.RemoveKeyword(AC_MQ03_HonorBound_MuniDowned_Keyword)
    Actor muni = muniRef as Actor
    If muni != None
        muni.SetUnconscious(False)
        muni.SetRestrained(False)
        muni.EvaluatePackage()
    EndIf
    MunisHelpedCount += 1
    If MunisHelpedCount >= MunisToHelpTotal
        SetStage(MunisHelpedStage)
    EndIf
EndFunction

Event OnQuestInit()
    RebuildMuniProgress()
    RegisterInteractionEvents()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    RebuildMuniProgress()
    RegisterInteractionEvents()
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Alias_Player.GetReference()
        Return
    EndIf
    If akSender == BackDoorActivator.GetReference()
        HandleBackDoorActivation()
    ElseIf IsValve(akSender)
        HandleValveActivation(akSender)
    Else
        HelpMuni(akSender)
    EndIf
EndEvent
