Function Fragment_Stage_0010_Item_00()
    If Alias_Marker_ShinCave != None && Alias_Marker_ShinCave.GetReference() == None && Alias_Markers_RaiderCave_Shin != None && Alias_Markers_RaiderCave_Shin.GetCount() > 0
        ObjectReference shinMarker = Alias_Markers_RaiderCave_Shin.GetAt(0)
        If shinMarker != None
            Alias_Marker_ShinCave.ForceRefTo(shinMarker)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor player = Game.GetPlayer()
    If Alias_Player != None
        Alias_Player.ForceRefIfEmpty(player)
        Actor aliasPlayer = Alias_Player.GetActorReference()
        If aliasPlayer != None
            player = aliasPlayer
        EndIf
    EndIf

    If player != None
        If BS01_ShinAwayValue != None
            player.SetValue(BS01_ShinAwayValue, 1.0)
        EndIf
        If BS01_PierceAwayValue != None
            player.SetValue(BS01_PierceAwayValue, 1.0)
        EndIf
        If BS01_AV_IsInitiate != None && BS01_MQ01_Trust != None && BS01_MQ01_Trust.IsCompleted()
            player.SetValue(BS01_AV_IsInitiate, 1.0)
        EndIf
    EndIf

    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
    If MakeshiftVaultMapMarker != None
        MakeshiftVaultMapMarker.AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    ObjectReference enableMarker = Alias_EnableMarker_ShinCrew.GetReference()
    If enableMarker != None
        enableMarker.Enable()
    EndIf

    Actor shin = Alias_Actor_Shin_RaiderCave.GetActorReference()
    If shin != None
        shin.EvaluatePackage()
    EndIf

    If BS01_MQ06A_Raiders_05_Negotiation != None && !BS01_MQ06A_Raiders_05_Negotiation.IsPlaying()
        BS01_MQ06A_Raiders_05_Negotiation.Start()
    EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None && BS01_MQ05_Raiders_AV_Negotiation != None
        player.SetValue(BS01_MQ05_Raiders_AV_Negotiation, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0520_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None && BS01_MQ05_Raiders_AV_Negotiation != None
        player.SetValue(BS01_MQ05_Raiders_AV_Negotiation, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_0530_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None && BS01_MQ05_Raiders_AV_Negotiation != None
        player.SetValue(BS01_MQ05_Raiders_AV_Negotiation, 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0540_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None && BS01_MQ05_Raiders_AV_Negotiation != None
        player.SetValue(BS01_MQ05_Raiders_AV_Negotiation, 4.0)
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None && BS01_MQ05_Raiders_AV_Negotiation != None
        player.SetValue(BS01_MQ05_Raiders_AV_Negotiation, 5.0)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    Actor pierce = Alias_Actor_Pierce_RaiderCave.GetActorReference()
    If pierce != None
        pierce.EvaluatePackage()
    EndIf
    If Alias_Actors_RaiderCrew != None
        Int index = 0
        While index < Alias_Actors_RaiderCrew.GetCount()
            Actor raider = Alias_Actors_RaiderCrew.GetAt(index) as Actor
            If raider != None
                raider.EvaluatePackage()
            EndIf
            index += 1
        EndWhile
    EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
    If BS01_MQ06A_Raiders_05_Negotiation != None && BS01_MQ06A_Raiders_05_Negotiation.IsPlaying()
        BS01_MQ06A_Raiders_05_Negotiation.Stop()
    EndIf

    Actor shin = Alias_Actor_Shin_RaiderCave.GetActorReference()
    Actor pierce = Alias_Actor_Pierce_RaiderCave.GetActorReference()
    Actor raider01 = Alias_Actor_Raider01.GetActorReference()
    Actor raider02 = Alias_Actor_Raider02.GetActorReference()

    If pierce != None
        If CaptiveFaction != None
            pierce.RemoveFromFaction(CaptiveFaction)
        EndIf
        If BS01_MQ05_Raiders_EnemyFaction != None
            pierce.AddToFaction(BS01_MQ05_Raiders_EnemyFaction)
        EndIf
        If shin != None
            pierce.StartCombat(shin, True)
        EndIf
    EndIf
    If raider01 != None
        If CaptiveFaction != None
            raider01.RemoveFromFaction(CaptiveFaction)
        EndIf
        If BS01_MQ05_Raiders_EnemyFaction != None
            raider01.AddToFaction(BS01_MQ05_Raiders_EnemyFaction)
        EndIf
        If shin != None
            raider01.StartCombat(shin, True)
        EndIf
    EndIf
    If raider02 != None
        If CaptiveFaction != None
            raider02.RemoveFromFaction(CaptiveFaction)
        EndIf
        If BS01_MQ05_Raiders_EnemyFaction != None
            raider02.AddToFaction(BS01_MQ05_Raiders_EnemyFaction)
        EndIf
        If shin != None
            raider02.StartCombat(shin, True)
        EndIf
    EndIf

    If Alias_Actors_RaiderCrew != None
        Int index = 0
        While index < Alias_Actors_RaiderCrew.GetCount()
            Actor raider = Alias_Actors_RaiderCrew.GetAt(index) as Actor
            If raider != None
                If CaptiveFaction != None
                    raider.RemoveFromFaction(CaptiveFaction)
                EndIf
                If BS01_MQ05_Raiders_EnemyFaction != None
                    raider.AddToFaction(BS01_MQ05_Raiders_EnemyFaction)
                EndIf
                If shin != None
                    raider.StartCombat(shin, True)
                EndIf
            EndIf
            index += 1
        EndWhile
    EndIf

    If Alias_Actors_BoSCrew != None
        Int index = 0
        While index < Alias_Actors_BoSCrew.GetCount()
            Actor crewMember = Alias_Actors_BoSCrew.GetAt(index) as Actor
            If crewMember != None
                crewMember.EvaluatePackage()
            EndIf
            index += 1
        EndWhile
    EndIf
EndFunction

Function Fragment_Stage_0655_Item_00()
    If IsStageDone(660) && !IsStageDone(665)
        SetStage(665)
    EndIf
EndFunction

Function Fragment_Stage_0660_Item_00()
    If IsStageDone(655) && !IsStageDone(665)
        SetStage(665)
    EndIf
EndFunction

Function Fragment_Stage_0665_Item_00()
    Actor pierce = Alias_Actor_Pierce_RaiderCave.GetActorReference()
    If pierce != None
        pierce.StopCombat()
        pierce.StopCombatAlarm()
        If BS01_MQ05_Raiders_EnemyFaction != None
            pierce.RemoveFromFaction(BS01_MQ05_Raiders_EnemyFaction)
        EndIf
        If CaptiveFaction != None
            pierce.AddToFaction(CaptiveFaction)
        EndIf
        ObjectReference downedMarker = Alias_Marker_PierceDowned.GetReference()
        If downedMarker != None
            pierce.MoveTo(downedMarker)
        EndIf
    EndIf

    If Alias_Actors_BoSCrew != None
        Int index = 0
        While index < Alias_Actors_BoSCrew.GetCount()
            Actor crewMember = Alias_Actors_BoSCrew.GetAt(index) as Actor
            If crewMember != None
                crewMember.StopCombat()
                crewMember.StopCombatAlarm()
            EndIf
            index += 1
        EndWhile
    EndIf

    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(55)
    If BS01_MQ06A_Raiders_06_ShinAfterCombat != None && !BS01_MQ06A_Raiders_06_ShinAfterCombat.IsPlaying()
        BS01_MQ06A_Raiders_06_ShinAfterCombat.Start()
    EndIf
EndFunction

Function Fragment_Stage_0670_Item_00()
    SetObjectiveDisplayed(55)
EndFunction

Function Fragment_Stage_0680_Item_00()
    SetObjectiveCompleted(55)
    Actor shin = Alias_Actor_Shin_RaiderCave.GetActorReference()
    If shin != None
        shin.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    If BS01_MQ06A_Raiders_05_Negotiation != None && BS01_MQ06A_Raiders_05_Negotiation.IsPlaying()
        BS01_MQ06A_Raiders_05_Negotiation.Stop()
    EndIf
    If BS01_MQ06A_Raiders_06_ShinAfterCombat != None && BS01_MQ06A_Raiders_06_ShinAfterCombat.IsPlaying()
        BS01_MQ06A_Raiders_06_ShinAfterCombat.Stop()
    EndIf

    Actor shin = Alias_Actor_Shin_RaiderCave.GetActorReference()
    If shin != None
        ObjectReference relogMarker = Alias_Marker_RaiderCave_ShinRelog.GetReference()
        If relogMarker != None
            shin.MoveTo(relogMarker)
        EndIf
        shin.EvaluatePackage()
    EndIf

    SetObjectiveCompleted(20)
    If IsObjectiveDisplayed(55)
        SetObjectiveCompleted(55)
    EndIf
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0800_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None
        If BS01_ShinAwayValue != None
            player.SetValue(BS01_ShinAwayValue, 0.0)
        EndIf
        If BS01_PierceAwayValue != None
            player.SetValue(BS01_PierceAwayValue, 0.0)
        EndIf
    EndIf

    ObjectReference enableMarker = Alias_EnableMarker_ShinCrew.GetReference()
    If enableMarker != None
        enableMarker.Disable()
    EndIf

    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0825_Item_00()
    If BS01_MQ05_Raiders_WarRoomAmbient != None && !BS01_MQ05_Raiders_WarRoomAmbient.IsPlaying()
        BS01_MQ05_Raiders_WarRoomAmbient.Start()
    EndIf
EndFunction

Function Fragment_Stage_0850_Item_00()
    If BS01_MQ05_Raiders_WarRoomAmbient != None && BS01_MQ05_Raiders_WarRoomAmbient.IsPlaying()
        BS01_MQ05_Raiders_WarRoomAmbient.Stop()
    EndIf
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0900_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None && BS01_AV_MetPierce != None
        player.SetValue(BS01_AV_MetPierce, 1.0)
    EndIf
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0950_Item_00()
    If BS01_MQ05_Raiders_WarRoomAmbient != None && BS01_MQ05_Raiders_WarRoomAmbient.IsPlaying()
        BS01_MQ05_Raiders_WarRoomAmbient.Stop()
    EndIf
    SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None && BS01_MQ06A_Raiders_SheenasHolotape != None
        ObjectReference holotapeRef = Alias_QO_Holotape_Sheenas.GetReference()
        If holotapeRef == None
            holotapeRef = player.PlaceAtMe(BS01_MQ06A_Raiders_SheenasHolotape, 1, True, True, False)
            If holotapeRef != None
                Alias_QO_Holotape_Sheenas.ForceRefTo(holotapeRef)
            EndIf
        EndIf
        If holotapeRef != None && holotapeRef.GetContainer() != player
            player.AddItem(holotapeRef, 1, True)
        ElseIf holotapeRef == None && player.GetItemCount(BS01_MQ06A_Raiders_SheenasHolotape) < 1
            player.AddItem(BS01_MQ06A_Raiders_SheenasHolotape, 1, True)
        EndIf
    EndIf

    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(90)
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_1050_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None && BS01_Valdez_Terminal_Password != None && player.GetItemCount(BS01_Valdez_Terminal_Password) < 1
        player.AddItem(BS01_Valdez_Terminal_Password, 1, True)
    EndIf
    SetObjectiveCompleted(100)
EndFunction

Function Fragment_Stage_1100_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None && BS01_MQ05_Raiders_AV_HelpedSheena != None
        player.SetValue(BS01_MQ05_Raiders_AV_HelpedSheena, 1.0)
    EndIf
    If !IsStageDone(1190)
        SetStage(1190)
    EndIf
EndFunction

Function Fragment_Stage_1150_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None && BS01_MQ05_Raiders_AV_HelpedSheena != None
        player.SetValue(BS01_MQ05_Raiders_AV_HelpedSheena, 0.0)
    EndIf
    If !IsStageDone(1190)
        SetStage(1190)
    EndIf
EndFunction

Function Fragment_Stage_1190_Item_00()
    SetObjectiveCompleted(90)
    If !IsStageDone(1050) && IsObjectiveDisplayed(100)
        SetObjectiveFailed(100)
    EndIf
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_1200_Item_00()
    Actor player = Alias_Player.GetActorReference()
    ObjectReference holotapeRef = Alias_QO_Holotape_Sheenas.GetReference()
    If player != None
        If holotapeRef != None && holotapeRef.GetContainer() == player
            player.RemoveItem(holotapeRef, 1, True)
        ElseIf BS01_MQ06A_Raiders_SheenasHolotape != None && player.GetItemCount(BS01_MQ06A_Raiders_SheenasHolotape) > 0
            player.RemoveItem(BS01_MQ06A_Raiders_SheenasHolotape, 1, True)
        EndIf
    EndIf
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_1210_Item_00()
    SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_1250_Item_00()
    SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_1280_Item_00()
    If IsStageDone(1290) && !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1290_Item_00()
    If IsStageDone(1280) && !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor player = Alias_Player.GetActorReference()
    If player != None
        If BS01_ShinAwayValue != None
            player.SetValue(BS01_ShinAwayValue, 0.0)
        EndIf
        If BS01_PierceAwayValue != None
            player.SetValue(BS01_PierceAwayValue, 0.0)
        EndIf
    EndIf

    SetObjectiveCompleted(120)

    RetrySettlersHandoff()
EndFunction

Function RetrySettlersHandoff()
    Bool handoffAccepted = False
    If BS01_MQ06B_Settlers != None
        handoffAccepted = BS01_MQ06B_Settlers.IsRunning() || BS01_MQ06B_Settlers.IsCompleted()
    EndIf

    If !handoffAccepted && BS01_MQ06B_Settlers_QuestStartKeyword != None
        Actor player = Alias_Player.GetActorReference()
        If player == None
            player = Game.GetPlayer()
        EndIf
        If player != None
            handoffAccepted = BS01_MQ06B_Settlers_QuestStartKeyword.SendStoryEventAndWait(None, player, player)
        EndIf
    EndIf

    If !handoffAccepted && BS01_MQ06B_Settlers != None
        handoffAccepted = BS01_MQ06B_Settlers.IsRunning() || BS01_MQ06B_Settlers.IsCompleted()
    EndIf

    If handoffAccepted
        Stop()
    Else
        StartTimer(5.0, 9000)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 9000 && IsRunning() && IsStageDone(9000)
        RetrySettlersHandoff()
    EndIf
EndEvent
