Function Fragment_Stage_0001_Item_00()
    ObjectReference noteRef = None
    ObjectReference noteMarkerRef = None
    If Alias_LouNote != None
        noteRef = Alias_LouNote.GetReference()
    EndIf
    If Alias_LouNoteMarker != None
        noteMarkerRef = Alias_LouNoteMarker.GetReference()
    EndIf
    If noteRef != None
        noteRef.EnableNoWait()
    EndIf
    If noteMarkerRef != None
        noteMarkerRef.EnableNoWait()
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    Actor koganRef = None
    If Alias_Kogan != None
        koganRef = Alias_Kogan.GetActorReference()
    EndIf
    If koganRef != None
        koganRef.EnableNoWait()
        koganRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
    Actor weaselRef = None
    Actor louRef = None
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If Alias_Lou != None
        louRef = Alias_Lou.GetActorReference()
    EndIf
    If weaselRef != None
        weaselRef.EnableNoWait()
        weaselRef.EvaluatePackage()
    EndIf
    If louRef != None
        louRef.EnableNoWait()
        louRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
    Actor gailRef = None
    If Alias_Gail != None
        gailRef = Alias_Gail.GetActorReference()
    EndIf
    If gailRef != None
        gailRef.EnableNoWait()
        gailRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If !IsStageDone(1)
        SetStage(1)
    EndIf
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0210_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_MQR_201P_FisherLieValue != None
        playerRef.SetValue(W05_MQR_201P_FisherLieValue, 0.0)
    EndIf
EndFunction

Function Fragment_Stage_0211_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_MQR_201P_FisherLieValue != None
        playerRef.SetValue(W05_MQR_201P_FisherLieValue, 1.0)
    EndIf
    SetObjectiveCompleted(200)
    If !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && SuperStimpak != None
        playerRef.AddItem(SuperStimpak, 1, True)
    EndIf
    If playerRef != None && W05_MQR_201P_FisherStimpakValue != None
        playerRef.SetValue(W05_MQR_201P_FisherStimpakValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0410_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_MQR_201P_PlayerFisherTerminalValue != None
        playerRef.SetValue(W05_MQR_201P_PlayerFisherTerminalValue, 1.0)
    EndIf
    SetObjectiveCompleted(300)
    SetObjectiveCompleted(400)
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    If !IsStageDone(2)
        SetStage(2)
    EndIf
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0610_Item_00()
    SetObjectiveDisplayed(610)
EndFunction

Function Fragment_Stage_0615_Item_00()
    SetObjectiveCompleted(600)
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
    SetObjectiveCompleted(600)
    SetObjectiveCompleted(610)
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(800)
    SetObjectiveDisplayed(850)
    Actor playerRef = Game.GetPlayer()
    If W05_MQR_201P_Track_RadioQuestStartKeyword != None && playerRef != None
        W05_MQR_201P_Track_RadioQuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0860_Item_00()
    SetObjectiveCompleted(850)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
    Actor weaselRef = None
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If weaselRef != None
        weaselRef.EnableNoWait()
        weaselRef.EvaluatePackage()
    EndIf
    If W05_MQR_201P_Weasel_000_StandAndFacePlayer != None && !W05_MQR_201P_Weasel_000_StandAndFacePlayer.IsPlaying()
        W05_MQR_201P_Weasel_000_StandAndFacePlayer.Start()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveDisplayed(1100)
    Actor weaselRef = None
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If weaselRef != None
        weaselRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveDisplayed(1200)
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveDisplayed(1300)
    If W05_MQR_201P_Weasel_004_GoToWall01 != None && !W05_MQR_201P_Weasel_004_GoToWall01.IsPlaying()
        W05_MQR_201P_Weasel_004_GoToWall01.Start()
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveDisplayed(1400)
EndFunction

Function Fragment_Stage_1410_Item_00()
    If W05_MQR_201P_Weasel_006_BlowUpWall01 != None && !W05_MQR_201P_Weasel_006_BlowUpWall01.IsPlaying()
        W05_MQR_201P_Weasel_006_BlowUpWall01.Start()
    EndIf
EndFunction

Function Fragment_Stage_1420_Item_00()
    ObjectReference wallActivatorRef = None
    Actor weaselRef = None
    If Alias_WallActivator01 != None
        wallActivatorRef = Alias_WallActivator01.GetReference()
    EndIf
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If wallActivatorRef != None && weaselRef != None
        wallActivatorRef.Activate(weaselRef)
        weaselRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1430_Item_00()
    Actor weaselRef = None
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If weaselRef != None && W05_MQR_201P_Weasel_Deathtrap01_Comment01 != None
        weaselRef.Say(W05_MQR_201P_Weasel_Deathtrap01_Comment01, weaselRef, False, Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_1440_Item_00()
    Actor weaselRef = None
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If weaselRef != None && W05_MQR_201P_Weasel_Deathtrap02_Comment01 != None
        weaselRef.Say(W05_MQR_201P_Weasel_Deathtrap02_Comment01, weaselRef, False, Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    Actor weaselRef = None
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If weaselRef != None && W05_MQR_201P_Weasel_Deathtrap03_Comment01 != None
        weaselRef.Say(W05_MQR_201P_Weasel_Deathtrap03_Comment01, weaselRef, False, Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_1510_Item_00()
    ; FO76 respawn checkpoints are server/account-side; the placed trap remains locally active.
    Return
EndFunction

Function Fragment_Stage_1520_Item_00()
    Actor weaselRef = None
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If weaselRef != None && W05_MQR_201P_Weasel_Deathtrap03_Comment03A != None
        weaselRef.Say(W05_MQR_201P_Weasel_Deathtrap03_Comment03A, weaselRef, False, Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_1530_Item_00()
    ObjectReference checkpointRef = None
    Actor weaselRef = None
    If Alias_RespawnCheckPoint01 != None
        checkpointRef = Alias_RespawnCheckPoint01.GetReference()
    EndIf
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If weaselRef != None && checkpointRef != None
        weaselRef.MoveTo(checkpointRef)
        weaselRef.EvaluatePackage()
    EndIf
    If weaselRef != None && W05_MQR_201P_Weasel_Deathtrap03_Comment03B != None
        weaselRef.Say(W05_MQR_201P_Weasel_Deathtrap03_Comment03B, weaselRef, False, Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    If W05_MQR_201P_Weasel_007_BlowUpWall02 != None && !W05_MQR_201P_Weasel_007_BlowUpWall02.IsPlaying()
        W05_MQR_201P_Weasel_007_BlowUpWall02.Start()
    EndIf
EndFunction

Function Fragment_Stage_1620_Item_00()
    ObjectReference wallActivatorRef = None
    Actor weaselRef = None
    If Alias_WallActivator02 != None
        wallActivatorRef = Alias_WallActivator02.GetReference()
    EndIf
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If wallActivatorRef != None && weaselRef != None
        wallActivatorRef.Activate(weaselRef)
        weaselRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveDisplayed(1700)
    Actor weaselRef = None
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If weaselRef != None && W05_MQR_201P_Weasel_ApproachingLou != None
        weaselRef.Say(W05_MQR_201P_Weasel_ApproachingLou, weaselRef, False, Game.GetPlayer())
        weaselRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1705_Item_00()
    SetObjectiveDisplayed(1705)
EndFunction

Function Fragment_Stage_1740_Item_00()
    If IsObjectiveDisplayed(1705) && !IsObjectiveCompleted(1705)
        SetObjectiveFailed(1705)
    EndIf
EndFunction

Function Fragment_Stage_1745_Item_00()
    If IsObjectiveDisplayed(1705) && !IsObjectiveCompleted(1705) && !IsObjectiveFailed(1705)
        SetObjectiveCompleted(1705)
    EndIf
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveDisplayed(1800)
EndFunction

Function Fragment_Stage_1801_Item_00()
    If IsObjectiveDisplayed(1800) && !IsObjectiveCompleted(1800)
        SetObjectiveCompleted(1800)
    EndIf
EndFunction

Function Fragment_Stage_1810_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_MQR_LouPromiseValue != None
        playerRef.SetValue(W05_MQR_LouPromiseValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveCompleted(1700)
    If IsObjectiveDisplayed(1800) && !IsObjectiveCompleted(1800)
        SetObjectiveCompleted(1800)
    EndIf
    SetObjectiveDisplayed(1900)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_MQR_201P_CompletedMineValue != None
        playerRef.SetValue(W05_MQR_201P_CompletedMineValue, 1.0)
    EndIf
    Actor weaselRef = None
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If weaselRef != None
        weaselRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1901_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_MQR_201P_ToldMegAboutLouValue != None
        playerRef.SetValue(W05_MQR_201P_ToldMegAboutLouValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1910_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_MQR_GailAwayValue != None
        playerRef.SetValue(W05_MQR_GailAwayValue, 0.0)
    EndIf
    Actor gailRef = None
    If Alias_Gail != None
        gailRef = Alias_Gail.GetActorReference()
    EndIf
    If gailRef != None
        gailRef.EnableNoWait()
        gailRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(1900)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_MQR_201P_CompletedMineValue != None
        playerRef.SetValue(W05_MQR_201P_CompletedMineValue, 1.0)
    EndIf
    If playerRef != None && Reputation_AV_Crater != None && Rep_Mod_Add_MQ != None
        playerRef.ModValue(Reputation_AV_Crater, Rep_Mod_Add_MQ.GetValue())
    EndIf
    ; FO76 random reward delivery is server/account-side and has no deterministic FO4 grant.
    If W05_MQR_202P_QuestStart_Keyword != None && playerRef != None
        W05_MQR_202P_QuestStart_Keyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    If W05_MQR_201P_Track_RadioQuest != None && W05_MQR_201P_Track_RadioQuest.IsRunning()
        W05_MQR_201P_Track_RadioQuest.SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
    If W05_MQR_201P_Track_RadioQuest != None && W05_MQR_201P_Track_RadioQuest.IsRunning()
        W05_MQR_201P_Track_RadioQuest.SetStage(1000)
    EndIf
EndFunction
