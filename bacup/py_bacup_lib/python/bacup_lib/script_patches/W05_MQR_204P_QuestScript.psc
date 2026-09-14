Actor Function GetQuestPlayer()
    Actor playerRef = None
    If currentPlayer != None
        playerRef = currentPlayer.GetActorReference()
    EndIf
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Return playerRef
EndFunction

Function AddFreeLouPerk()
    Actor playerRef = GetQuestPlayer()
    If playerRef != None && W05_MQR_204P_FreeLou_Perk != None && !playerRef.HasPerk(W05_MQR_204P_FreeLou_Perk)
        playerRef.AddPerk(W05_MQR_204P_FreeLou_Perk)
    EndIf
EndFunction

Function RemoveFreeLouPerk()
    Actor playerRef = GetQuestPlayer()
    If playerRef != None && W05_MQR_204P_FreeLou_Perk != None && playerRef.HasPerk(W05_MQR_204P_FreeLou_Perk)
        playerRef.RemovePerk(W05_MQR_204P_FreeLou_Perk)
    EndIf
EndFunction

Function SyncFreeLouPerk()
    If IsStageDone(FreeLouStage) && !IsStageDone(TalkToLouStage)
        AddFreeLouPerk()
    Else
        RemoveFreeLouPerk()
    EndIf
EndFunction

Function EnsureVault79CodeNote()
    Actor playerRef = GetQuestPlayer()
    If playerRef == None
        Return
    EndIf

    If W05_MQ00_CodeAV != None && playerRef.GetValue(W05_MQ00_CodeAV) < 0.0
        playerRef.SetValue(W05_MQ00_CodeAV, Utility.RandomInt(100000, 999999))
    EndIf
    If W05_MQR_Vault79CodeNote != None && playerRef.GetItemCount(W05_MQR_Vault79CodeNote) == 0
        playerRef.AddItem(W05_MQR_Vault79CodeNote, 1, False)
    EndIf
EndFunction

Function RegisterForPlayerLoad()
    Actor playerRef = GetQuestPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndFunction

Function UnregisterForPlayerLoad()
    Actor playerRef = GetQuestPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndFunction

Event OnQuestInit()
    RegisterForPlayerLoad()
    EnsureVault79CodeNote()
    SyncFreeLouPerk()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 100
        EnsureVault79CodeNote()
    ElseIf auiStageID == FreeLouStage
        AddFreeLouPerk()
    ElseIf auiStageID >= TalkToLouStage
        RemoveFreeLouPerk()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        EnsureVault79CodeNote()
        SyncFreeLouPerk()
    EndIf
EndEvent

Event OnQuestShutdown()
    RemoveFreeLouPerk()
    UnregisterForPlayerLoad()
EndEvent

Event OnReset()
    RemoveFreeLouPerk()
    UnregisterForPlayerLoad()
EndEvent
