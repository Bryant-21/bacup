Function Fragment_Stage_0050_Item_00()
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If !IsStageDone(200)
        SetStage(200)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    If !IsObjectiveCompleted(200) && !IsObjectiveDisplayed(200)
        SetObjectiveDisplayed(200)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If !IsObjectiveCompleted(200)
        SetObjectiveCompleted(200)
    EndIf
    If !IsObjectiveCompleted(400) && !IsObjectiveDisplayed(400)
        SetObjectiveDisplayed(400)
    EndIf
EndFunction

Function Fragment_Stage_0490_Item_00()
    If !IsObjectiveCompleted(400)
        SetObjectiveCompleted(400)
    EndIf
    If !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    ObjectReference bossChest = Alias_RollinsBossChest.GetReference()
    Actor player = Alias_Player.GetActorReference()
    If !player
        player = Game.GetPlayer()
        If player
            Alias_Player.ForceRefIfEmpty(player)
        EndIf
    EndIf

    If bossChest && Key_Jail && (!player || player.GetItemCount(Key_Jail) <= 0) && bossChest.GetItemCount(Key_Jail) <= 0
        ObjectReference jailKey = Alias_JailKeys.GetReference()
        If !jailKey
            jailKey = bossChest.PlaceAtMe(Key_Jail, 1, True, True, False)
            If jailKey
                Alias_JailKeys.ForceRefTo(jailKey)
            EndIf
        EndIf
        If jailKey
            bossChest.AddItem(jailKey, 1, True)
        Else
            bossChest.AddItem(Key_Jail, 1, True)
        EndIf
    EndIf

    If !IsObjectiveCompleted(500) && !IsObjectiveDisplayed(500)
        SetObjectiveDisplayed(500)
    EndIf
EndFunction

Function Fragment_Stage_0590_Item_00()
    If !IsObjectiveCompleted(500)
        SetObjectiveCompleted(500)
    EndIf
    If !IsStageDone(595)
        SetStage(595)
    EndIf
EndFunction

Function Fragment_Stage_0595_Item_00()
    B21:LocalEncounterMaterializer materializer = (Self as Quest) as B21:LocalEncounterMaterializer
    If materializer
        materializer.PrepareEligibleWaves()
    EndIf
    DefaultQuestEncounterWaveScript encounterWaves = (Self as Quest) as DefaultQuestEncounterWaveScript
    If encounterWaves
        encounterWaves.StartLocalEncounterWave(0)
    EndIf
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    If !IsObjectiveCompleted(600) && !IsObjectiveDisplayed(600)
        SetObjectiveDisplayed(600)
    EndIf
EndFunction

Function Fragment_Stage_0690_Item_00()
    If !IsObjectiveCompleted(600)
        SetObjectiveCompleted(600)
    EndIf
    Actor beckett = Alias_Beckett.GetActorReference()
    If beckett
        beckett.EvaluatePackage()
    EndIf
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    If !IsObjectiveCompleted(700) && !IsObjectiveDisplayed(700)
        SetObjectiveDisplayed(700)
    EndIf
EndFunction

Function Fragment_Stage_0790_Item_00()
    If !IsObjectiveCompleted(700)
        SetObjectiveCompleted(700)
    EndIf
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    If !IsObjectiveCompleted(1000) && !IsObjectiveDisplayed(1000)
        SetObjectiveDisplayed(1000)
    EndIf
EndFunction

Function Fragment_Stage_1090_Item_00()
    If !IsObjectiveCompleted(1000)
        SetObjectiveCompleted(1000)
    EndIf
    If !IsStageDone(2000)
        SetStage(2000)
    EndIf
EndFunction

Function Fragment_Stage_2000_Item_00()
    If !IsObjectiveCompleted(2000) && !IsObjectiveDisplayed(2000)
        SetObjectiveDisplayed(2000)
    EndIf
EndFunction

Function Fragment_Stage_2090_Item_00()
    If !IsObjectiveCompleted(2000)
        SetObjectiveCompleted(2000)
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    Stop()
EndFunction
