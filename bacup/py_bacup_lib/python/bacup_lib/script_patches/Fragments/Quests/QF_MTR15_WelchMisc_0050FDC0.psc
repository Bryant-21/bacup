Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(MTR15_MiscComplete, 1.0)
    EndIf
    CompleteAllObjectives()
    ; B21:QuestRewards / CurrencyQuestRewards pay on OnStageSet; let it dispatch before shutdown.
    Utility.Wait(1.0)
    Stop()
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction
