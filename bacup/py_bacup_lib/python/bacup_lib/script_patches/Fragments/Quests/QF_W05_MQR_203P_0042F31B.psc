Function Fragment_Stage_0002_Item_00()
    ObjectReference disabledMarker = Alias_DisabledForQuestEnableMarker.GetReference()
    ObjectReference enabledMarker = Alias_EnabledForQuestEnableMarker.GetReference()
    ObjectReference entranceMarker = ArenaEntranceMarker.GetReference()
    ObjectReference entranceDoor = Alias_EntranceDoor.GetReference()
    If disabledMarker != None
        disabledMarker.Disable()
    EndIf
    If enabledMarker != None
        enabledMarker.Enable()
    EndIf
    If entranceMarker != None
        entranceMarker.Enable()
    EndIf
    If entranceDoor != None
        entranceDoor.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
    ; FO4 lacks the FO76 proximity callback that continued registration.
    If !IsStageDone(405)
        SetStage(405)
    EndIf
EndFunction

Function Fragment_Stage_0405_Item_00()
    If W05_MQR_203P_Johnny_002B_RegistrationScene != None
        W05_MQR_203P_Johnny_002B_RegistrationScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0410_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.ModValue(pW05_MQR_JohnnyRelationshipValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0420_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.ModValue(pW05_MQR_JohnnyRelationshipValue, -1.0)
    EndIf
EndFunction

Function Fragment_Stage_0499_Item_00()
    If W05_MQR_203P_Johnny_002C_RegistrationScene != None
        W05_MQR_203P_Johnny_002C_RegistrationScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0510_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.SetValue(W05_MQR_203P_DignityValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    If W05_MQR_203P_SargentoPA_002_Round01GhouldenBoy != None
        W05_MQR_203P_SargentoPA_002_Round01GhouldenBoy.Start()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0605_Item_00()
    If W05_MQR_203P_SargentoPA_003_Round01CallPlayer != None
        W05_MQR_203P_SargentoPA_003_Round01CallPlayer.Start()
    EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
    If W05_MQR_203P_GuardArena_001_Round01 != None
        W05_MQR_203P_GuardArena_001_Round01.Start()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0710_Item_00()
    ObjectReference arenaDoor = Alias_EntranceDoor.GetReference()
    If arenaDoor != None
        arenaDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(799)
    SetObjectiveDisplayed(800)
    If W05_MQR_203P_SargentoPA_004_Round01PlayerArena != None
        W05_MQR_203P_SargentoPA_004_Round01PlayerArena.Start()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_01()
    ; The local controller replaces FO76's server encounter service.
    DefaultQuestEncounterWaveScript waveScript = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waveScript != None
        waveScript.StartLocalEncounterWave(0)
    EndIf
EndFunction

Function Fragment_Stage_0810_Item_00()
    SetObjectiveDisplayed(850)
    If W05_MQR_203P_SargentoPA_004B_Round01End != None
        W05_MQR_203P_SargentoPA_004B_Round01End.Start()
    EndIf
EndFunction

Function Fragment_Stage_0815_Item_00()
    Actor donRef = Alias_Don.GetActorReference()
    Actor guardRef = Alias_GuardArena.GetActorReference()
    If donRef != None
        donRef.EvaluatePackage()
    EndIf
    If guardRef != None
        guardRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0820_Item_00()
    Actor donRef = Alias_Don.GetActorReference()
    If donRef != None && !donRef.IsDead()
        donRef.Kill()
    EndIf
EndFunction

Function Fragment_Stage_0822_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.ModValue(pW05_MQR_JohnnyRelationshipValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
    If W05_MQR_203P_Johnny_EnterLockerRoom != None
        W05_MQR_203P_Johnny_EnterLockerRoom.Start()
    EndIf
EndFunction

Function Fragment_Stage_0901_Item_00()
    If W05_MQR_203P_SargentoPA_001_Idle != None
        W05_MQR_203P_SargentoPA_001_Idle.Start()
    EndIf
EndFunction

Function Fragment_Stage_0910_Item_00()
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
    SetObjectiveDisplayed(950)
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
    If !IsStageDone(1050)
        SetStage(1050)
    EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
    SetObjectiveCompleted(1000)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveDisplayed(1100)
    If W05_MQR_203P_SargentoPA_005_Round02CallPlayer != None
        W05_MQR_203P_SargentoPA_005_Round02CallPlayer.Start()
    EndIf
EndFunction

Function Fragment_Stage_1110_Item_00()
    ObjectReference arenaDoor = Alias_EntranceDoor.GetReference()
    If arenaDoor != None
        arenaDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveDisplayed(1199)
    SetObjectiveDisplayed(1200)
    If W05_MQR_203P_SargentoPA_006_Round02PlayerArena != None
        W05_MQR_203P_SargentoPA_006_Round02PlayerArena.Start()
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_01()
    ; The local controller replaces FO76's server encounter service.
    DefaultQuestEncounterWaveScript waveScript = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waveScript != None
        waveScript.StartLocalEncounterWave(1)
    EndIf
EndFunction

Function Fragment_Stage_1205_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    Actor neutralTurretActor
    Int turretIndex = 0
    ObjectReference neutralTurret
    ObjectReference allyTurret
    If playerRef != None && playerRef.GetValue(W05_MQR_203P_Round2CheatValue) == 2.0
        If W05_MQR_203P_SargentoPA_011_Turrets != None
            W05_MQR_203P_SargentoPA_011_Turrets.Start()
        EndIf
        While turretIndex < Alias_NeutralTurrets.GetCount()
            neutralTurret = Alias_NeutralTurrets.GetAt(turretIndex)
            If neutralTurret != None
                ; FO4 has no FO76 turret-faction swap service, so teammate state is the local substitute.
                neutralTurret.Enable()
                neutralTurretActor = neutralTurret as Actor
                If neutralTurretActor != None
                    neutralTurretActor.SetPlayerTeammate(True, False, False)
                EndIf
                Alias_AllyTurrets.AddRef(neutralTurret)
            EndIf
            turretIndex += 1
        EndWhile
        turretIndex = 0
        While turretIndex < Alias_AllyTurrets.GetCount()
            allyTurret = Alias_AllyTurrets.GetAt(turretIndex)
            If allyTurret != None
                allyTurret.Enable()
            EndIf
            turretIndex += 1
        EndWhile
    EndIf
EndFunction

Function Fragment_Stage_1210_Item_00()
    Actor allyTurretActor
    Int turretIndex = 0
    ObjectReference allyTurret
    SetObjectiveDisplayed(1250)
    While turretIndex < Alias_AllyTurrets.GetCount()
        allyTurret = Alias_AllyTurrets.GetAt(turretIndex)
        allyTurretActor = allyTurret as Actor
        If allyTurretActor != None
            allyTurretActor.SetPlayerTeammate(False, False, False)
        EndIf
        turretIndex += 1
    EndWhile
    If W05_MQR_203P_SargentoPA_006B_Round02End != None
        W05_MQR_203P_SargentoPA_006B_Round02End.Start()
    EndIf
EndFunction

Function Fragment_Stage_1215_Item_00()
    Actor klausRef = Alias_Klaus.GetActorReference()
    Actor guardRef = Alias_GuardArena.GetActorReference()
    If klausRef != None
        klausRef.EvaluatePackage()
    EndIf
    If guardRef != None
        guardRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1220_Item_00()
    Actor klausRef = Alias_Klaus.GetActorReference()
    If klausRef != None && !klausRef.IsDead()
        klausRef.Kill()
    EndIf
EndFunction

Function Fragment_Stage_1222_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.ModValue(pW05_MQR_JohnnyRelationshipValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveDisplayed(1300)
    If W05_MQR_203P_Johnny_EnterLockerRoom != None
        W05_MQR_203P_Johnny_EnterLockerRoom.Start()
    EndIf
EndFunction

Function Fragment_Stage_1301_Item_00()
    If W05_MQR_203P_SargentoPA_001_Idle != None
        W05_MQR_203P_SargentoPA_001_Idle.Start()
    EndIf
EndFunction

Function Fragment_Stage_1310_Item_00()
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1350_Item_00()
    SetObjectiveDisplayed(1350)
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveDisplayed(1400)
    If !IsStageDone(1450)
        SetStage(1450)
    EndIf
EndFunction

Function Fragment_Stage_1450_Item_00()
    SetObjectiveDisplayed(1450)
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveDisplayed(1500)
    If W05_MQR_203P_SargentoPA_007_Round03CallPlayer != None
        W05_MQR_203P_SargentoPA_007_Round03CallPlayer.Start()
    EndIf
EndFunction

Function Fragment_Stage_1510_Item_00()
    ObjectReference arenaDoor = Alias_EntranceDoor.GetReference()
    If arenaDoor != None
        arenaDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveDisplayed(1600)
    If W05_MQR_203P_SargentoPA_008_Round03PlayerArena != None
        W05_MQR_203P_SargentoPA_008_Round03PlayerArena.Start()
    EndIf
EndFunction

Function Fragment_Stage_1601_Item_00()
    W05_MQR_203P_QuestScript questScript = (Self as Quest) as W05_MQR_203P_QuestScript
    ObjectReference cageActivator
    If questScript != None && questScript.GraftonCageSequenceActivator != None
        cageActivator = questScript.GraftonCageSequenceActivator.GetReference()
    EndIf
    Actor playerRef = Game.GetPlayer()
    If cageActivator != None && playerRef != None
        cageActivator.Activate(playerRef)
        Utility.Wait(2.0)
        If !IsStageDone(1602)
            SetStage(1602)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1602_Item_00()
    W05_MQR_203P_QuestScript questScript = (Self as Quest) as W05_MQR_203P_QuestScript
    Actor playerRef = Game.GetPlayer()
    Actor graftonRef
    If questScript != None && questScript.Round03GraftonMonster != None
        graftonRef = questScript.Round03GraftonMonster.GetActorReference()
    EndIf
    If graftonRef != None && playerRef != None
        graftonRef.EvaluatePackage()
        graftonRef.StartCombat(playerRef, True)
    EndIf
    If !IsStageDone(1605)
        SetStage(1605)
    EndIf
EndFunction

Function Fragment_Stage_1605_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    Actor neutralTurretActor
    Int turretIndex = 0
    ObjectReference neutralTurret
    ObjectReference allyTurret
    If playerRef != None && playerRef.GetValue(W05_MQR_203P_Round3CheatValue) == 2.0
        If W05_MQR_203P_SargentoPA_011_Turrets != None
            W05_MQR_203P_SargentoPA_011_Turrets.Start()
        EndIf
        While turretIndex < Alias_NeutralTurrets.GetCount()
            neutralTurret = Alias_NeutralTurrets.GetAt(turretIndex)
            If neutralTurret != None
                ; FO4 has no FO76 turret-faction swap service, so teammate state is the local substitute.
                neutralTurret.Enable()
                neutralTurretActor = neutralTurret as Actor
                If neutralTurretActor != None
                    neutralTurretActor.SetPlayerTeammate(True, False, False)
                EndIf
                Alias_AllyTurrets.AddRef(neutralTurret)
            EndIf
            turretIndex += 1
        EndWhile
        turretIndex = 0
        While turretIndex < Alias_AllyTurrets.GetCount()
            allyTurret = Alias_AllyTurrets.GetAt(turretIndex)
            If allyTurret != None
                allyTurret.Enable()
            EndIf
            turretIndex += 1
        EndWhile
    EndIf
EndFunction

Function Fragment_Stage_1610_Item_00()
    Actor allyTurretActor
    Int turretIndex = 0
    ObjectReference allyTurret
    While turretIndex < Alias_AllyTurrets.GetCount()
        allyTurret = Alias_AllyTurrets.GetAt(turretIndex)
        allyTurretActor = allyTurret as Actor
        If allyTurretActor != None
            allyTurretActor.SetPlayerTeammate(False, False, False)
        EndIf
        turretIndex += 1
    EndWhile
    If !IsStageDone(1700)
        SetStage(1700)
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveDisplayed(1700)
    If W05_MQR_203P_SargentoPA_009A_Winner != None
        W05_MQR_203P_SargentoPA_009A_Winner.Start()
    EndIf
EndFunction

Function Fragment_Stage_1703_Item_00()
    Actor maddieRef = Alias_Maddie.GetActorReference()
    Actor guardRef = Alias_GuardArena.GetActorReference()
    If maddieRef != None
        maddieRef.EvaluatePackage()
    EndIf
    If guardRef != None
        guardRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1705_Item_00()
    Actor maddieRef = Alias_Maddie.GetActorReference()
    If maddieRef != None && !maddieRef.IsDead()
        maddieRef.Kill()
    EndIf
    If !IsStageDone(1715)
        SetStage(1715)
    EndIf
EndFunction

Function Fragment_Stage_1706_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.ModValue(pW05_MQR_JohnnyRelationshipValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1710_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.SetValue(W05_MQR_203P_SaleValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1715_Item_00()
    If W05_MQR_203P_Johnny_006_EnterArena != None
        W05_MQR_203P_Johnny_006_EnterArena.Start()
    EndIf
EndFunction

Function Fragment_Stage_1720_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetItemCount(W05_MQR_203P_HalRoomKey) < 1
        playerRef.AddItem(W05_MQR_203P_HalRoomKey, 1, False)
    EndIf
    If !IsStageDone(1750)
        SetStage(1750)
    EndIf
EndFunction

Function Fragment_Stage_1750_Item_00()
    If W05_MQR_203P_JohnnySargento_001_Winner != None
        W05_MQR_203P_JohnnySargento_001_Winner.Start()
    EndIf
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveDisplayed(1800)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_MQR_203P_WinnersCup_Blackout != None
        W05_MQR_203P_WinnersCup_Blackout.Cast(playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveDisplayed(1900)
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveDisplayed(2000)
EndFunction

Function Fragment_Stage_2010_Item_00()
    If W05_MQR_203P_Johnny_TravelToHal != None
        W05_MQR_203P_Johnny_TravelToHal.Start()
    EndIf
EndFunction

Function Fragment_Stage_2100_Item_00()
    SetObjectiveDisplayed(2100)
    If W05_MQR_203P_HalJohnny_001_Shoot != None
        W05_MQR_203P_HalJohnny_001_Shoot.Start()
    EndIf
EndFunction

Function Fragment_Stage_2110_Item_00()
    Actor halRef = Alias_Hal.GetActorReference()
    Actor johnnyRef = Alias_JohnnyArena.GetActorReference()
    If halRef != None && !halRef.IsDead()
        halRef.Kill(johnnyRef)
    EndIf
    If !IsStageDone(2200)
        SetStage(2200)
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
    SetObjectiveDisplayed(2200)
EndFunction

Function Fragment_Stage_2210_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.SetValue(W05_MQR_JohnnyCutValue, 50.0)
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_2220_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.SetValue(W05_MQR_JohnnyCutValue, 25.0)
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_2221_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.SetValue(W05_MQR_JohnnyCutValue, 25.0)
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_2300_Item_00()
    SetObjectiveDisplayed(2300)
EndFunction

Function Fragment_Stage_2400_Item_00()
    SetObjectiveDisplayed(2400)
EndFunction

Function Fragment_Stage_5000_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        If IsStageDone(1300)
            playerRef.SetValue(W05_MQR_203P_Round3CheatValue, 1.0)
        Else
            playerRef.SetValue(W05_MQR_203P_Round2CheatValue, 1.0)
        EndIf
    EndIf
    If W05_MQR_203P_JohnnyGuard_001_Distract != None
        W05_MQR_203P_JohnnyGuard_001_Distract.Start()
    EndIf
    If !IsStageDone(5100)
        SetStage(5100)
    EndIf
EndFunction

Function Fragment_Stage_5100_Item_00()
    SetObjectiveDisplayed(5100)
EndFunction

Function Fragment_Stage_5200_Item_00()
    SetObjectiveCompleted(5100)
    If IsStageDone(1300)
        If !IsStageDone(1310)
            SetStage(1310)
        EndIf
    ElseIf !IsStageDone(910)
        SetStage(910)
    EndIf
EndFunction

Function Fragment_Stage_5300_Item_00()
    SetObjectiveFailed(5100)
    If IsStageDone(1300)
        If !IsStageDone(1310)
            SetStage(1310)
        EndIf
    ElseIf !IsStageDone(910)
        SetStage(910)
    EndIf
EndFunction

Function Fragment_Stage_6000_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If IsStageDone(1300)
        If playerRef != None
            playerRef.SetValue(W05_MQR_203P_Round3CheatValue, 2.0)
        EndIf
        If !IsStageDone(1310)
            SetStage(1310)
        EndIf
    Else
        If playerRef != None
            playerRef.SetValue(W05_MQR_203P_Round2CheatValue, 2.0)
        EndIf
        If !IsStageDone(910)
            SetStage(910)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_7000_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.SetValue(W05_MQR_203P_Round3CheatValue, 3.0)
    EndIf
    If !IsStageDone(7100)
        SetStage(7100)
    EndIf
EndFunction

Function Fragment_Stage_7100_Item_00()
    SetObjectiveDisplayed(7100)
EndFunction

Function Fragment_Stage_7110_Item_00()
    If W05_MQR_203P_JohnnyMaddie_001_Convince != None
        W05_MQR_203P_JohnnyMaddie_001_Convince.Start()
    EndIf
EndFunction

Function Fragment_Stage_7120_Item_00()
    ObjectReference guardRef = Alias_GuardArena.GetReference()
    If guardRef != None
        Alias_GuardArenaName.ForceRefTo(guardRef)
    EndIf
EndFunction

Function Fragment_Stage_7200_Item_00()
    SetObjectiveCompleted(7100)
    If !IsStageDone(1310)
        SetStage(1310)
    EndIf
EndFunction

Function Fragment_Stage_7300_Item_00()
    SetObjectiveFailed(7100)
    If !IsStageDone(1310)
        SetStage(1310)
    EndIf
EndFunction

Function Fragment_Stage_8000_Item_00()
    If W05_MQR_203P_SargentoPA_010_PlanB != None
        W05_MQR_203P_SargentoPA_010_PlanB.Start()
    EndIf
EndFunction

Function Fragment_Stage_8100_Item_00()
    W05_MQR_203P_QuestScript questScript = (Self as Quest) as W05_MQR_203P_QuestScript
    Actor playerRef = Game.GetPlayer()
    Int attackerIndex = 0
    ObjectReference guardRef
    Actor attackerRef
    SetObjectiveDisplayed(8100)
    If questScript != None && questScript.Guards != None && questScript.PlanBAttackers != None && playerRef != None
        While attackerIndex < questScript.Guards.GetCount()
            guardRef = questScript.Guards.GetAt(attackerIndex)
            If guardRef != None
                questScript.PlanBAttackers.AddRef(guardRef)
            EndIf
            attackerRef = guardRef as Actor
            If attackerRef != None
                attackerRef.Enable()
                attackerRef.StartCombat(playerRef, True)
            EndIf
            attackerIndex += 1
        EndWhile
        questScript.Guards.EvaluateAll()
        questScript.PlanBAttackers.EvaluateAll()
    EndIf
EndFunction

Function Fragment_Stage_8110_Item_00()
    Actor sargentoRef = Alias_Sargento.GetActorReference()
    If sargentoRef != None
        sargentoRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_8200_Item_00()
    Actor johnnyRef = Alias_JohnnyArena.GetActorReference()
    SetObjectiveCompleted(8100)
    SetObjectiveDisplayed(8200)
    If johnnyRef != None
        johnnyRef.EvaluatePackage()
    EndIf
    If IsStageDone(8230) && !IsStageDone(8240)
        SetStage(8240)
    EndIf
EndFunction

Function Fragment_Stage_8210_Item_00()
    Actor johnnyRef = Alias_JohnnyArena.GetActorReference()
    If johnnyRef != None
        johnnyRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_8220_Item_00()
    If W05_MQR_203P_Johnny_TravelToHal != None
        W05_MQR_203P_Johnny_TravelToHal.Start()
    EndIf
EndFunction

Function Fragment_Stage_8230_Item_00()
    SetObjectiveCompleted(8200)
    If IsStageDone(8200) && !IsStageDone(8240)
        SetStage(8240)
    EndIf
EndFunction

Function Fragment_Stage_8250_Item_00()
    If W05_MQR_203P_HalJohnny_001_Shoot != None
        W05_MQR_203P_HalJohnny_001_Shoot.Start()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && Reputation_AV_Crater != None && Rep_Mod_Add_MQ != None
        playerRef.ModValue(Reputation_AV_Crater, Rep_Mod_Add_MQ.GetValue())
    EndIf
    If W05_MQR_Choice_QuestStartKeyword != None
        W05_MQR_Choice_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
    Stop()
EndFunction

Function Fragment_Stage_9001_Item_00()
    If IsStageDone(1600)
        SetObjectiveFailed(1600)
    ElseIf IsStageDone(1200)
        SetObjectiveFailed(1200)
    Else
        SetObjectiveFailed(800)
    EndIf
    If W05_MQR_203P_SargentoPA_009B_Loser != None
        W05_MQR_203P_SargentoPA_009B_Loser.Start()
    EndIf
    ; FO4 lost the mid-scene INFO fragment that starts Plan B; preserve scene order locally.
    Utility.Wait(2.0)
    If !IsStageDone(8000)
        SetStage(8000)
    EndIf
EndFunction

Function Fragment_Stage_9998_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && Collar != None
        playerRef.UnequipItem(Collar, False, True)
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    If IsStageDone(1600)
        SetObjectiveFailed(1600)
    ElseIf IsStageDone(1200)
        SetObjectiveFailed(1200)
    ElseIf IsStageDone(800)
        SetObjectiveFailed(800)
    EndIf
    Stop()
EndFunction

Function Fragment_Stage_10000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Actor allyTurretActor
    Int collarCount
    Int turretIndex = 0
    ObjectReference allyTurret
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && Collar != None
        collarCount = playerRef.GetItemCount(Collar)
        If collarCount > 0
            playerRef.RemoveItem(Collar, collarCount, True)
        EndIf
    EndIf
    ObjectReference disabledMarker = Alias_DisabledForQuestEnableMarker.GetReference()
    ObjectReference enabledMarker = Alias_EnabledForQuestEnableMarker.GetReference()
    If disabledMarker != None
        disabledMarker.Enable()
    EndIf
    If enabledMarker != None
        enabledMarker.Disable()
    EndIf
    While turretIndex < Alias_AllyTurrets.GetCount()
        allyTurret = Alias_AllyTurrets.GetAt(turretIndex)
        allyTurretActor = allyTurret as Actor
        If allyTurretActor != None
            allyTurretActor.SetPlayerTeammate(False, False, False)
        EndIf
        turretIndex += 1
    EndWhile
EndFunction
