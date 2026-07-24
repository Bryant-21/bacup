; TODO

Function Fragment_Stage_0001_Item_00()
    ObjectReference noteRef = None
    If Alias_LouNote != None
        noteRef = Alias_LouNote.GetReference()
    EndIf
    If noteRef != None
        noteRef.EnableNoWait()
    EndIf
    ObjectReference noteMarker = None
    If Alias_LouNoteMarker != None
        noteMarker = Alias_LouNoteMarker.GetReference()
    EndIf
    If noteMarker != None
        noteMarker.EnableNoWait()
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    Actor koganRef = None
    If Alias_Kogan != None
        koganRef = Alias_Kogan.GetActorReference()
    EndIf
    If koganRef != None
        koganRef.Enable()
        koganRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
    Actor weaselRef = None
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If weaselRef != None
        weaselRef.Enable()
        weaselRef.EvaluatePackage()
    EndIf
    Actor louRef = None
    If Alias_Lou != None
        louRef = Alias_Lou.GetActorReference()
    EndIf
    If louRef != None
        louRef.Enable()
        louRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
    Actor gailRef = None
    If Alias_Gail != None
        gailRef = Alias_Gail.GetActorReference()
    EndIf
    If gailRef != None
        gailRef.Enable()
        gailRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
    If !IsStageDone(1)
        SetStage(1)
    EndIf
    If !IsStageDone(200)
        SetStage(200)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0210_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_201P_FisherLieValue, 0.0)
    EndIf
    SetObjectiveCompleted(200)
    If !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0211_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_201P_FisherLieValue, 1.0)
    EndIf
    SetObjectiveCompleted(200)
    If !IsStageDone(300)
        SetStage(300)
    EndIf
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_201P_FisherStimpakValue, 1.0)
    EndIf
    SetObjectiveCompleted(200)
    If !IsStageDone(500)
        SetStage(500)
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
    If playerRef != None
        playerRef.SetValue(W05_MQR_201P_PlayerFisherTerminalValue, 1.0)
    EndIf
    SetObjectiveCompleted(300)
    SetObjectiveCompleted(400)
    If !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveCompleted(300)
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(500)
    If !IsStageDone(2)
        SetStage(2)
    EndIf
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500)
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
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(800)
    Actor playerRef = Game.GetPlayer()
    If W05_MQR_201P_Track_RadioQuestStartKeyword != None && playerRef != None
        W05_MQR_201P_Track_RadioQuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
    If !IsStageDone(860)
        SetStage(860)
    EndIf
EndFunction

Function Fragment_Stage_0860_Item_00()
    SetObjectiveCompleted(800)
    If !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
    If !IsStageDone(3)
        SetStage(3)
    EndIf
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(900)
    SetObjectiveDisplayed(1000)
    Actor weaselRef = None
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If weaselRef != None
        weaselRef.Enable()
        weaselRef.EvaluatePackage()
    EndIf
    If W05_MQR_201P_Weasel_000_StandAndFacePlayer != None && !W05_MQR_201P_Weasel_000_StandAndFacePlayer.IsPlaying()
        W05_MQR_201P_Weasel_000_StandAndFacePlayer.Start()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(1000)
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
    SetObjectiveCompleted(1100)
    SetObjectiveDisplayed(1200)
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(1200)
    SetObjectiveDisplayed(1300)
    If W05_MQR_201P_Weasel_004_GoToWall01 != None && !W05_MQR_201P_Weasel_004_GoToWall01.IsPlaying()
        W05_MQR_201P_Weasel_004_GoToWall01.Start()
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(1300)
    SetObjectiveDisplayed(1400)
EndFunction

Function Fragment_Stage_1410_Item_00()
    SetObjectiveCompleted(1400)
    If W05_MQR_201P_Weasel_006_BlowUpWall01 != None && !W05_MQR_201P_Weasel_006_BlowUpWall01.IsPlaying()
        W05_MQR_201P_Weasel_006_BlowUpWall01.Start()
    EndIf
EndFunction

Function Fragment_Stage_1420_Item_00()
    ObjectReference wallActivator = None
    Actor weaselRef = None
    If Alias_WallActivator01 != None
        wallActivator = Alias_WallActivator01.GetReference()
    EndIf
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If wallActivator != None && weaselRef != None
        wallActivator.Activate(weaselRef)
    EndIf
    If !IsStageDone(1600)
        SetStage(1600)
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    If W05_MQR_201P_Weasel_007_BlowUpWall02 != None && !W05_MQR_201P_Weasel_007_BlowUpWall02.IsPlaying()
        W05_MQR_201P_Weasel_007_BlowUpWall02.Start()
    EndIf
EndFunction

Function Fragment_Stage_1620_Item_00()
    ObjectReference wallActivator = None
    Actor weaselRef = None
    If Alias_WallActivator02 != None
        wallActivator = Alias_WallActivator02.GetReference()
    EndIf
    If Alias_Weasel != None
        weaselRef = Alias_Weasel.GetActorReference()
    EndIf
    If wallActivator != None && weaselRef != None
        wallActivator.Activate(weaselRef)
    EndIf
    If !IsStageDone(1700)
        SetStage(1700)
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveDisplayed(1700)
    If !IsStageDone(1705)
        SetStage(1705)
    EndIf
EndFunction

Function Fragment_Stage_1705_Item_00()
    SetObjectiveDisplayed(1705)
    If !IsStageDone(1740)
        SetStage(1740)
    EndIf
EndFunction

Function Fragment_Stage_1740_Item_00()
    If !IsStageDone(1800)
        SetStage(1800)
    EndIf
EndFunction

Function Fragment_Stage_1745_Item_00()
    SetObjectiveCompleted(1705)
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveCompleted(1700)
    SetObjectiveDisplayed(1800)
EndFunction

Function Fragment_Stage_1801_Item_00()
    SetObjectiveCompleted(1800)
EndFunction

Function Fragment_Stage_1810_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_LouPromiseValue, 1.0)
    EndIf
    If !IsStageDone(1900)
        SetStage(1900)
    EndIf
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveDisplayed(1900)
    If !IsStageDone(4)
        SetStage(4)
    EndIf
EndFunction

Function Fragment_Stage_1901_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_201P_ToldMegAboutLouValue, 1.0)
    EndIf
    If !IsStageDone(1910)
        SetStage(1910)
    EndIf
EndFunction

Function Fragment_Stage_1910_Item_00()
    Actor gailRef = None
    If Alias_Gail != None
        gailRef = Alias_Gail.GetActorReference()
    EndIf
    If gailRef != None
        gailRef.Enable()
        gailRef.EvaluatePackage()
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(1900)
    Actor playerRef = Game.GetPlayer()
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
    Stop()
EndFunction
