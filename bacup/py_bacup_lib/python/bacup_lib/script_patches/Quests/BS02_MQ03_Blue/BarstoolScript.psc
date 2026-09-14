Event OnAliasInit()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && GetReference() != None
        RegisterForRemoteEvent(playerRef, "OnSit")
    EndIf
EndEvent

Event Actor.OnSit(Actor akSender, ObjectReference akFurniture)
    Quest owningQuest = GetOwningQuest()
    Actor playerRef = Alias_Player.GetActorReference()
    If owningQuest == None || !owningQuest.IsObjectiveDisplayed(35) || owningQuest.IsObjectiveCompleted(35) || playerRef == None || akSender != playerRef || akFurniture != GetReference()
        Return
    EndIf

    playerRef.SetValue(BS02_MQ03_Blue_Barstool_AV, 1.0)
    If Scene_DrinkTalk != None && !Scene_DrinkTalk.IsPlaying()
        Scene_DrinkTalk.Start()
    EndIf
EndEvent
