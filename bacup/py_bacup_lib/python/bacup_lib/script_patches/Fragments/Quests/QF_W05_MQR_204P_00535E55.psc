Function Fragment_Stage_0002_Item_00()
    If Alias_InitEnableMarker != None
        ObjectReference initMarker = Alias_InitEnableMarker.GetReference()
        If initMarker != None
            initMarker.Enable()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
    W05_MQR_204P_QuestScript questScript = (Self as Quest) as W05_MQR_204P_QuestScript
    If questScript == None
        Return
    EndIf

    If questScript.LevHideoutEnableMarker != None
        ObjectReference enableMarker = questScript.LevHideoutEnableMarker.GetReference()
        If enableMarker != None
            enableMarker.Enable()
        EndIf
    EndIf
    If questScript.LevHideoutEncounterEnableMarker != None
        ObjectReference encounterMarker = questScript.LevHideoutEncounterEnableMarker.GetReference()
        If encounterMarker != None
            encounterMarker.Enable()
        EndIf
    EndIf
    If questScript.LevHideoutLayoutEnableMarker != None
        ObjectReference layoutMarker = questScript.LevHideoutLayoutEnableMarker.GetReference()
        If layoutMarker != None
            layoutMarker.Enable()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
    If Alias_Rocco != None
        ObjectReference roccoRef = Alias_Rocco.GetReference()
        If roccoRef != None
            roccoRef.Enable()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_204P_Started, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_0151_Item_00()
    SetObjectiveCompleted(150)
EndFunction

Function Fragment_Stage_0160_Item_00()
    SetObjectiveCompleted(150)
    SetObjectiveDisplayed(160)
    If TiedUpScene != None
        TiedUpScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(160)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    If TiedUpScene != None
        TiedUpScene.Stop()
    EndIf
    If GetUpScene != None
        GetUpScene.Start()
    EndIf
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(300)
    If Alias_Lou != None
        Actor louRef = Alias_Lou.GetActorReference()
        If louRef != None
            louRef.EvaluatePackage()
            If W05_MQR_204P_Lou_Freed != None
                louRef.Say(W05_MQR_204P_Lou_Freed)
            EndIf
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveCompleted(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(500)
    SetObjectiveDisplayed(510)
    SetObjectiveDisplayed(520)
    SetObjectiveDisplayed(530)
    SetObjectiveDisplayed(540)
EndFunction

Function Fragment_Stage_0510_Item_00()
    SetObjectiveCompleted(510)
EndFunction

Function Fragment_Stage_0511_Item_00()
    SetObjectiveDisplayed(510)
EndFunction

Function Fragment_Stage_0512_Item_00()
    SetObjectiveDisplayed(510)
EndFunction

Function Fragment_Stage_0513_Item_00()
    SetObjectiveCompleted(510)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0520_Item_00()
    SetObjectiveCompleted(520)
EndFunction

Function Fragment_Stage_0521_Item_00()
    SetObjectiveDisplayed(520)
EndFunction

Function Fragment_Stage_0522_Item_00()
    SetObjectiveDisplayed(520)
EndFunction

Function Fragment_Stage_0530_Item_00()
    SetObjectiveCompleted(530)
EndFunction

Function Fragment_Stage_0531_Item_00()
    SetObjectiveDisplayed(530)
EndFunction

Function Fragment_Stage_0532_Item_00()
    SetObjectiveDisplayed(530)
EndFunction

Function Fragment_Stage_0533_Item_00()
    SetObjectiveCompleted(530)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0540_Item_00()
    SetObjectiveCompleted(540)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_204P_SaboteurKnownValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0541_Item_00()
    SetObjectiveDisplayed(540)
    ; FO76 Raider reputation is account-backed; stage 541 remains the local threat flag.
EndFunction

Function Fragment_Stage_0550_Item_00()
    If Alias_Barb == None
        Return
    EndIf

    Actor barbRef = Alias_Barb.GetActorReference()
    Actor playerRef = Game.GetPlayer()
    If barbRef == None || playerRef == None
        Return
    EndIf
    If CaptiveFaction != None
        barbRef.RemoveFromFaction(CaptiveFaction)
    EndIf
    If BoundCaptiveFaction != None
        barbRef.RemoveFromFaction(BoundCaptiveFaction)
    EndIf
    If PlayerEnemyFaction != None
        barbRef.AddToFaction(PlayerEnemyFaction)
    EndIf
    barbRef.StartCombat(playerRef, True)
EndFunction

Function Fragment_Stage_0560_Item_00()
    SetObjectiveCompleted(520)
    SetObjectiveCompleted(600)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_204P_KillBarbValue, 1.0)
    EndIf
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(700)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_204P_LevHideoutActiveValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0810_Item_00()
    If IsStageDone(820) && !IsStageDone(830)
        SetStage(830)
    EndIf
EndFunction

Function Fragment_Stage_0820_Item_00()
    If IsStageDone(810) && !IsStageDone(830)
        SetStage(830)
    EndIf
EndFunction

Function Fragment_Stage_0830_Item_00()
    SetObjectiveCompleted(800)
    SetObjectiveDisplayed(840)
EndFunction

Function Fragment_Stage_0840_Item_00()
    SetObjectiveCompleted(840)
EndFunction

Function Fragment_Stage_0850_Item_00()
    If Alias_Lev == None
        Return
    EndIf

    Actor levRef = Alias_Lev.GetActorReference()
    If levRef == None
        Return
    EndIf
    levRef.StopCombat()
    levRef.EvaluatePackage()
    SetStage(900)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
EndFunction

Function Fragment_Stage_0950_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_204P_SaboteurKnownValue, 1.0)
    EndIf
    If !IsStageDone(5000)
        SetStage(5000)
    EndIf
EndFunction

Function Fragment_Stage_0960_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_204P_LevManipulatedLouValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0970_Item_00()
    If Alias_Lev == None
        Return
    EndIf

    Actor levRef = Alias_Lev.GetActorReference()
    Actor playerRef = Game.GetPlayer()
    If levRef == None || playerRef == None
        Return
    EndIf
    If CaptiveFaction != None
        levRef.RemoveFromFaction(CaptiveFaction)
    EndIf
    If BoundCaptiveFaction != None
        levRef.RemoveFromFaction(BoundCaptiveFaction)
    EndIf
    If PlayerEnemyFaction != None
        levRef.AddToFaction(PlayerEnemyFaction)
    EndIf
    levRef.StartCombat(playerRef, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(900)
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(1000)
    SetObjectiveDisplayed(1100)
EndFunction

Function Fragment_Stage_1110_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_204P_ToldMegAboutBarbValue, 1.0)
    EndIf
    ; FO76 Raider reputation is account-backed; the local dialogue flag is preserved above.
EndFunction

Function Fragment_Stage_1120_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_204P_ToldMegAboutRoccoValue, 1.0)
    EndIf
    ; FO76 Raider reputation is account-backed; the local dialogue flag is preserved above.
EndFunction

Function Fragment_Stage_5000_Item_00()
    SetObjectiveDisplayed(5000)
EndFunction

Function Fragment_Stage_5100_Item_00()
    SetObjectiveCompleted(5000)
    SetObjectiveDisplayed(5100)
EndFunction

Function Fragment_Stage_5200_Item_00()
    SetObjectiveCompleted(5100)
    SetObjectiveDisplayed(5200)
EndFunction

Function Fragment_Stage_5210_Item_00()
    SetObjectiveCompleted(5200)
    If IsObjectiveDisplayed(5300)
        SetObjectiveCompleted(5300)
    EndIf
EndFunction

Function Fragment_Stage_5300_Item_00()
    SetObjectiveCompleted(5200)
    SetObjectiveDisplayed(5300)
EndFunction

Function Fragment_Stage_5310_Item_00()
    SetObjectiveCompleted(5300)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_204P_KillRoccoValue, 1.0)
    EndIf
    If !IsStageDone(5210)
        SetStage(5210)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(1100)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQ_204P_FactionChosen, 1.0)
    EndIf
    If W05_MQR_205P_QuestStart_Keyword != None
        W05_MQR_205P_QuestStart_Keyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveCompleted(150)
    SetObjectiveCompleted(160)
    SetObjectiveCompleted(200)
    SetObjectiveCompleted(300)
    SetObjectiveCompleted(400)
    SetObjectiveCompleted(500)
    SetObjectiveCompleted(510)
    SetObjectiveCompleted(520)
    SetObjectiveCompleted(530)
    SetObjectiveCompleted(540)
    SetObjectiveCompleted(600)
    SetObjectiveCompleted(700)
    SetObjectiveCompleted(800)
    SetObjectiveCompleted(840)
    SetObjectiveCompleted(900)
    SetObjectiveCompleted(1000)
    SetObjectiveCompleted(1100)
    SetObjectiveCompleted(5000)
    SetObjectiveCompleted(5100)
    SetObjectiveCompleted(5200)
    SetObjectiveCompleted(5300)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_204P_LevHideoutActiveValue, 0.0)
    EndIf
    If TiedUpScene != None
        TiedUpScene.Stop()
    EndIf
    If GetUpScene != None
        GetUpScene.Stop()
    EndIf
EndFunction
