Actor Function ReturnPlayerReference()
    If PlayerRefAlias != None
        Actor aliasedPlayer = PlayerRefAlias.GetActorReference()
        If aliasedPlayer != None
            Return aliasedPlayer
        EndIf
    EndIf
    Return Game.GetPlayer()
EndFunction

Bool Function StageGateIsOpen(Int aiStageIndex)
    If StagesProperties == None || aiStageIndex < 0 || aiStageIndex >= StagesProperties.Length
        Return True
    EndIf
    Int prereq = StagesProperties[aiStageIndex].iPreReqStage
    Return prereq <= 0 || IsStageDone(prereq)
EndFunction

Function AdvanceStageForIndex(Int aiStageIndex)
    If StagesProperties == None || aiStageIndex < 0 || aiStageIndex >= StagesProperties.Length
        Return
    EndIf
    Int stageToSet = StagesProperties[aiStageIndex].iStageToSet
    If stageToSet > 0 && !IsStageDone(stageToSet)
        iCurrentStageIndex = aiStageIndex
        SetStage(stageToSet)
    EndIf
EndFunction

Function WatchReturnActivators()
    If ActivatorsAndTeleport == None
        Return
    EndIf
    Int index = 0
    While index < ActivatorsAndTeleport.Length
        ReferenceAlias activatorAlias = ActivatorsAndTeleport[index].ActivatorAlias
        If activatorAlias != None && activatorAlias.GetReference() != None
            RegisterForRemoteEvent(activatorAlias.GetReference(), "OnActivate")
        EndIf
        index += 1
    EndWhile
EndFunction

Event OnQuestInit()
    WatchReturnActivators()
    Actor player = ReturnPlayerReference()
    If player != None
        RegisterForRemoteEvent(player, "OnPlayerLoadGame")
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    WatchReturnActivators()
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    Actor player = ReturnPlayerReference()
    If player == None || akActionRef != player || ActivatorsAndTeleport == None
        Return
    EndIf
    Int index = 0
    While index < ActivatorsAndTeleport.Length
        ReferenceAlias activatorAlias = ActivatorsAndTeleport[index].ActivatorAlias
        If activatorAlias != None && activatorAlias.GetReference() == akSender
            Int stageIndex = ActivatorsAndTeleport[index].iStageIndex
            If StageGateIsOpen(stageIndex)
                ReferenceAlias destinationAlias = ActivatorsAndTeleport[index].ObjectToTPto
                ObjectReference destination = None
                If destinationAlias != None
                    destination = destinationAlias.GetReference()
                EndIf
                If destination != None
                    player.MoveTo(destination)
                    AdvanceStageForIndex(stageIndex)
                EndIf
            EndIf
            Return
        EndIf
        index += 1
    EndWhile
EndEvent

Event OnQuestShutdown()
    Actor player = ReturnPlayerReference()
    If player != None
        UnregisterForRemoteEvent(player, "OnPlayerLoadGame")
    EndIf
    If ActivatorsAndTeleport == None
        Return
    EndIf
    Int index = 0
    While index < ActivatorsAndTeleport.Length
        ReferenceAlias activatorAlias = ActivatorsAndTeleport[index].ActivatorAlias
        If activatorAlias != None && activatorAlias.GetReference() != None
            UnregisterForRemoteEvent(activatorAlias.GetReference(), "OnActivate")
        EndIf
        index += 1
    EndWhile
EndEvent
