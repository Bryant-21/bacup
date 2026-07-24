Event OnQuestInit()
    If BlockadeTriggerAlias != None && BlockadeTriggerAlias.GetReference() != None
        RegisterForRemoteEvent(BlockadeTriggerAlias.GetReference(), "OnTriggerEnter")
    EndIf
    If GreetTriggerAlias != None && GreetTriggerAlias.GetReference() != None
        RegisterForRemoteEvent(GreetTriggerAlias.GetReference(), "OnTriggerEnter")
    EndIf
    If GreetTrigger02Alias != None && GreetTrigger02Alias.GetReference() != None
        RegisterForRemoteEvent(GreetTrigger02Alias.GetReference(), "OnTriggerEnter")
    EndIf
    If TrespassDiaTriggerAlias != None && TrespassDiaTriggerAlias.GetReference() != None
        RegisterForRemoteEvent(TrespassDiaTriggerAlias.GetReference(), "OnTriggerEnter")
    EndIf
    If TrespassDiaTriggerAlias02 != None && TrespassDiaTriggerAlias02.GetReference() != None
        RegisterForRemoteEvent(TrespassDiaTriggerAlias02.GetReference(), "OnTriggerEnter")
    EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If currentPlayer == None || akActionRef != currentPlayer.GetReference()
        Return
    EndIf
    If currentPlayer.GetActorReference() == None
        Return
    EndIf

    LetPlayerPass = False
    If W05_Community_RaiderBlockade_CanPass != None
        LetPlayerPass = (currentPlayer.GetActorReference().GetValue(W05_Community_RaiderBlockade_CanPass) >= 1.0)
    EndIf

    If LetPlayerPass
        If W05_Community_RaiderBlockade_Faction != None
            currentPlayer.GetActorReference().AddToFaction(W05_Community_RaiderBlockade_Faction)
        EndIf
    Else
        If W05_Community_RaiderBlockadeEnemy_Faction != None
            currentPlayer.GetActorReference().AddToFaction(W05_Community_RaiderBlockadeEnemy_Faction)
        EndIf
    EndIf
EndEvent
