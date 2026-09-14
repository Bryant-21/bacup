Function SayLocalTopic(Topic akTopic)
    Actor machineVoice = GenericMachineVoice.GetActorReference()
    If machineVoice != None && akTopic != None
        machineVoice.Say(akTopic)
    EndIf
EndFunction

Bool Function HasLocalSiloAccess(Actor akPlayer)
    If EN05_MQ_Officer != None && (EN05_MQ_Officer.IsCompleted() || EN05_MQ_Officer.IsStageDone(110))
        Return True
    EndIf
    Return akPlayer != None && EN05_MQ_CompletedValue != None && akPlayer.GetValue(EN05_MQ_CompletedValue) >= 1.0
EndFunction

Event OnAliasInit()
    ObjectReference accessPanel = GetReference()
    If accessPanel != None
        accessPanel.BlockActivation(True, False)
    EndIf
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player
        Return
    EndIf
    If !HasLocalSiloAccess(player)
        SayLocalTopic(EN07_AccessDenied)
        Return
    EndIf
    ObjectReference accessPanel = GetReference()
    ObjectReference invisibleDoor
    If accessPanel != None
        invisibleDoor = accessPanel.GetLinkedRef(EN07_LinkInvisibleDoorKeyword)
    EndIf
    If invisibleDoor != None
        invisibleDoor.Lock(False, False)
        invisibleDoor.SetOpen(True)
    EndIf
    MSiloPermitEntryKeyword.SendStoryEventAndWait(player.GetCurrentLocation(), player, accessPanel)
EndEvent
