Actor Function ActorFromAlias(ReferenceAlias actorAlias)
    If actorAlias == None
        Return None
    EndIf
    Return actorAlias.GetActorReference()
EndFunction

Actor Function PlayerReference()
    Return ActorFromAlias(PlayerAlias)
EndFunction

AC_MQ04_Sins_QuestScript Function QuestController()
    Return (Self as Quest) as AC_MQ04_Sins_QuestScript
EndFunction

Function StartSceneIfStopped(Scene sceneToStart)
    If sceneToStart != None && !sceneToStart.IsPlaying()
        sceneToStart.Start()
    EndIf
EndFunction

Function StopSceneIfPlaying(Scene sceneToStop)
    If sceneToStop != None && sceneToStop.IsPlaying()
        sceneToStop.Stop()
    EndIf
EndFunction

Function EnableAlias(ReferenceAlias targetAlias)
    If targetAlias == None
        Return
    EndIf
    ObjectReference target = targetAlias.GetReference()
    If target != None
        target.Enable()
        Actor targetActor = target as Actor
        If targetActor != None
            targetActor.EvaluatePackage()
        EndIf
    EndIf
EndFunction

Function EnableCollection(RefCollectionAlias targets, Bool attackPlayer = False)
    Int index = 0
    Actor player = PlayerReference()
    While index < targets.GetCount()
        ObjectReference target = targets.GetAt(index)
        If target != None
            target.Enable()
            Actor targetActor = target as Actor
            If targetActor != None
                targetActor.EvaluatePackage()
                If attackPlayer && player != None
                    targetActor.StartCombat(player)
                EndIf
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function DisableCollection(RefCollectionAlias targets)
    Int index = 0
    While index < targets.GetCount()
        ObjectReference target = targets.GetAt(index)
        If target != None
            target.Disable()
        EndIf
        index += 1
    EndWhile
EndFunction

Function MoveActorToMarker(ReferenceAlias actorAlias, ReferenceAlias markerAlias)
    Actor targetActor = ActorFromAlias(actorAlias)
    ObjectReference marker = markerAlias.GetReference()
    If targetActor != None && marker != None
        targetActor.Enable()
        targetActor.MoveTo(marker)
        targetActor.EvaluatePackage()
    EndIf
EndFunction

Function SwapPheromone(ReferenceAlias activatorAlias, ReferenceAlias staticAlias, Scene reactionScene, Int stageId)
    ObjectReference activatorRef = activatorAlias.GetReference()
    ObjectReference staticRef = staticAlias.GetReference()
    If activatorRef != None
        activatorRef.Disable()
    EndIf
    If staticRef != None
        staticRef.Enable()
    EndIf
    StartSceneIfStopped(reactionScene)
    AC_MQ04_Sins_QuestScript controller = QuestController()
    If controller != None
        controller.RecordPheromonePlaced(stageId)
    EndIf
EndFunction

Function SetTravelActorsInAtlanticCity(Bool inAtlanticCity)
    Actor antonioMansion = ActorFromAlias(Alias_AntonioMansion)
    Actor antonioAtlantic = ActorFromAlias(AntonioCity)
    Actor abbieMansion = ActorFromAlias(Alias_AbbieMansion)
    Actor abbieAtlantic = ActorFromAlias(AbbieCity)
    If inAtlanticCity
        If antonioMansion != None
            antonioMansion.SetValue(AV_AntonioAway, 1.0)
        EndIf
        If antonioAtlantic != None
            antonioAtlantic.SetValue(AV_AntonioACAway, 0.0)
            antonioAtlantic.Enable()
            antonioAtlantic.EvaluatePackage()
        EndIf
        If abbieMansion != None
            abbieMansion.SetValue(AbbieAway, 1.0)
        EndIf
        If abbieAtlantic != None
            abbieAtlantic.SetValue(AV_AbbieACAway, 0.0)
            abbieAtlantic.Enable()
            abbieAtlantic.EvaluatePackage()
        EndIf
    Else
        If antonioMansion != None
            antonioMansion.SetValue(AV_AntonioAway, 0.0)
            antonioMansion.Enable()
            antonioMansion.EvaluatePackage()
        EndIf
        If antonioAtlantic != None
            antonioAtlantic.SetValue(AV_AntonioACAway, 1.0)
        EndIf
        If abbieMansion != None
            abbieMansion.SetValue(AbbieAway, 0.0)
            abbieMansion.Enable()
            abbieMansion.EvaluatePackage()
        EndIf
        If abbieAtlantic != None
            abbieAtlantic.SetValue(AV_AbbieACAway, 1.0)
        EndIf
    EndIf
EndFunction

Function GiveIfMissing(Form itemToGive)
    Actor player = PlayerReference()
    If player != None && itemToGive != None && player.GetItemCount(itemToGive) < 1
        player.AddItem(itemToGive, 1, True)
    EndIf
EndFunction

Function RemoveAll(Form itemToRemove)
    Actor player = PlayerReference()
    If player != None && itemToRemove != None
        Int count = player.GetItemCount(itemToRemove)
        If count > 0
            player.RemoveItem(itemToRemove, count, True)
        EndIf
    EndIf
EndFunction

Function PrepareFinaleActors()
    MoveActorToMarker(Alias_AntonioMansion, Alias_Marker_AntonioFinale)
    If IsStageDone(300)
        MoveActorToMarker(Alias_AbbieMansion, Alias_Marker_AbbieFinale)
    EndIf
    MoveActorToMarker(Alias_Evelyn, Alias_Marker_EvelynFinale)
    MoveActorToMarker(Alias_Vin, Alias_Marker_VinFinale)
EndFunction

Function SetFinaleActorPresence()
    Actor antonioMansion = ActorFromAlias(Alias_AntonioMansion)
    Actor antonioAtlantic = ActorFromAlias(AntonioCity)
    Actor abbieMansion = ActorFromAlias(Alias_AbbieMansion)
    Actor abbieAtlantic = ActorFromAlias(AbbieCity)
    If antonioMansion != None
        antonioMansion.SetValue(AV_AntonioAway, 0.0)
        antonioMansion.Enable()
        antonioMansion.EvaluatePackage()
    EndIf
    If antonioAtlantic != None
        antonioAtlantic.SetValue(AV_AntonioACAway, 1.0)
    EndIf
    If IsStageDone(300)
        If abbieMansion != None
            abbieMansion.SetValue(AbbieAway, 0.0)
            abbieMansion.Enable()
            abbieMansion.EvaluatePackage()
        EndIf
        If abbieAtlantic != None
            abbieAtlantic.SetValue(AV_AbbieACAway, 1.0)
        EndIf
    Else
        If abbieMansion != None
            abbieMansion.SetValue(AbbieAway, 1.0)
        EndIf
        If abbieAtlantic != None
            If IsStageDone(320)
                abbieAtlantic.SetValue(AV_AbbieACAway, 0.0)
            Else
                abbieAtlantic.SetValue(AV_AbbieACAway, 1.0)
            EndIf
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetTravelActorsInAtlanticCity(False)
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0105_Item_00()
    SetTravelActorsInAtlanticCity(False)
    EnableAlias(Alias_Vin)
    EnableAlias(Alias_Evelyn)
EndFunction

Function Fragment_Stage_0106_Item_00()
    SetTravelActorsInAtlanticCity(True)
    EnableAlias(Gene)
EndFunction

Function Fragment_Stage_0107_Item_00()
    SetFinaleActorPresence()
    PrepareFinaleActors()
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0111_Item_00()
    Actor antonio = ActorFromAlias(Alias_AntonioMansion)
    If antonio != None
        antonio.RemoveKeyword(AnimArchetypeElderly)
        antonio.SetValue(AC_MQ_AntonioSenility_AV, 0.0)
        antonio.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0112_Item_00()
    StartSceneIfStopped(ConfrontationScene)
EndFunction

Function Fragment_Stage_0113_Item_00()
    StartSceneIfStopped(ConfrontationScene3)
EndFunction

Function Fragment_Stage_0114_Item_00()
    EnableAlias(Alias_AntonioMansion)
EndFunction

Function Fragment_Stage_0115_Item_00()
    SetObjectiveCompleted(15)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0116_Item_00()
    MoveActorToMarker(Alias_AntonioMansion, ConfrontSceneMarker_Antonio)
    MoveActorToMarker(Alias_AbbieMansion, ConfrontSceneMarker_Abbie)
    MoveActorToMarker(Alias_Evelyn, ConfrontSceneMarker_Evelyn)
    MoveActorToMarker(Alias_Vin, ConfrontSceneMarker_Vin)
EndFunction

Function Fragment_Stage_0117_Item_00()
    StartSceneIfStopped(AC_MQ04_Sins_DevilReveal)
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(18)
    SetObjectiveDisplayed(25)
    GiveIfMissing(BackRoomKey)
    GiveIfMissing(AntoniosTerminalKey)
EndFunction

Function Fragment_Stage_0125_Item_00()
    SetObjectiveCompleted(18)
    SetObjectiveCompleted(25)
    SetObjectiveDisplayed(30)
    SetTravelActorsInAtlanticCity(True)
EndFunction

Function Fragment_Stage_0130_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(35)
    EnableAlias(ChemLabEntrance)
    ObjectReference chemLab = ChemLabEntrance.GetReference()
    If chemLab != None
        chemLab.BlockActivation(False)
        chemLab.Lock(False)
    EndIf
    DisableCollection(DisableCivilians)
    EnableCollection(HostileMobsters, True)
EndFunction

Function Fragment_Stage_0132_Item_00()
    EnableAlias(Gene)
    StartSceneIfStopped(PreGeneMeetScene)
EndFunction

Function Fragment_Stage_0135_Item_00()
    SetObjectiveCompleted(35)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0140_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0145_Item_00()
    EnableCollection(HostileMobsters, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SwapPheromone(PheromoneActivator01, PheromoneStatic01, AntonioPheromones01, 150)
EndFunction

Function Fragment_Stage_0152_Item_00()
    SwapPheromone(PheromoneActivator02, PheromoneStatic02, AntonioPheromones02, 152)
EndFunction

Function Fragment_Stage_0154_Item_00()
    SwapPheromone(PheromoneActivator03, PheromoneStatic03, AntonioPheromones03, 154)
EndFunction

Function Fragment_Stage_0156_Item_00()
    SwapPheromone(PheromoneActivator04, PheromoneStatic04, AntonioPheromones04, 156)
EndFunction

Function Fragment_Stage_0158_Item_00()
    SwapPheromone(PheromoneActivator05, PheromoneStatic05, AntonioPheromones05, 158)
EndFunction

Function Fragment_Stage_0160_Item_00()
    SetObjectiveCompleted(45)
    SetObjectiveDisplayed(50)
    StartSceneIfStopped(AntonioAllPheromones)
EndFunction

Function Fragment_Stage_0163_Item_00()
    Actor player = PlayerReference()
    If JDRoar != None && player != None
        JDRoar.Play(player)
    EndIf
EndFunction

Function Fragment_Stage_0165_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(55)
    MoveActorToMarker(AntonioCity, Alias_AntonioBossFightMarker)
EndFunction

Function Fragment_Stage_0167_Item_00()
    If !IsStageDone(170)
        SetStage(170)
    EndIf
EndFunction

Function Fragment_Stage_0170_Item_00()
    SetObjectiveCompleted(55)
    SetObjectiveDisplayed(60)
    EnableAlias(JerseyDevil)
    Actor antonio = ActorFromAlias(AntonioCity)
    If antonio != None
        antonio.AddToFaction(PlayerAllyFaction)
        antonio.EvaluatePackage()
    EndIf
    AC_MQ04_Sins_QuestScript controller = QuestController()
    If controller != None
        controller.BeginJerseyDevilFight()
    EndIf
EndFunction

Function Fragment_Stage_0171_Item_00()
    EnableAlias(JerseyDevil)
    If EncWavesMessage != None
        EncWavesMessage.Show()
    EndIf
EndFunction

Function Fragment_Stage_0172_Item_00()
    EnableAlias(JerseyDevil)
EndFunction

Function Fragment_Stage_0173_Item_00()
    EnableAlias(JerseyDevil)
EndFunction

Function Fragment_Stage_0174_Item_00()
    EnableAlias(JerseyDevil)
EndFunction

Function Fragment_Stage_0175_Item_00()
    EnableAlias(JerseyDevil)
EndFunction

Function Fragment_Stage_0176_Item_00()
    EnableAlias(JerseyDevil)
EndFunction

Function Fragment_Stage_0180_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(65)
    AC_MQ04_Sins_QuestScript controller = QuestController()
    If controller != None
        controller.DownJerseyDevil()
    EndIf
EndFunction

Function Fragment_Stage_0185_Item_00()
    SetObjectiveCompleted(65)
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0190_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(75)
    Actor player = PlayerReference()
    If player != None && ActionExtractBlood != None
        player.PlayIdleAction(ActionExtractBlood, None)
    EndIf
    ObjectReference bloodTrigger = JerseyDevilBloodTrigger.GetReference()
    If bloodTrigger != None && BloodExplosion != None
        bloodTrigger.PlaceAtMe(BloodExplosion)
    EndIf
    GiveIfMissing(DevilsBloodVial)
EndFunction

Function Fragment_Stage_0195_Item_00()
    Actor player = PlayerReference()
    If player != None
        player.SetValue(PlayerBloodChoice, 1.0)
    EndIf
    AC_MQ04_Sins_QuestScript controller = QuestController()
    If controller != None
        controller.ReleaseJerseyDevil()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    Actor player = PlayerReference()
    If player != None
        player.SetValue(PlayerBloodChoice, 2.0)
    EndIf
    AC_MQ04_Sins_QuestScript controller = QuestController()
    If controller != None
        controller.ReleaseJerseyDevil()
    EndIf
EndFunction

Function Fragment_Stage_0201_Item_00()
    Actor player = PlayerReference()
    ObjectReference destination = PlayerTeleportMarker.GetReference()
    If player != None && destination != None
        player.MoveTo(destination)
    EndIf
    EnableAlias(Gene)
    StartSceneIfStopped(PreGeneReturnScene)
EndFunction

Function Fragment_Stage_0205_Item_00()
    SetObjectiveCompleted(75)
    SetObjectiveDisplayed(80)
    EnableAlias(AntonioCity)
EndFunction

Function Fragment_Stage_0210_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(85)
    EnableAlias(Gene)
EndFunction

Function Fragment_Stage_0213_Item_00()
    If !IsStageDone(215)
        SetStage(215)
    EndIf
EndFunction

Function Fragment_Stage_0215_Item_00()
    SetObjectiveCompleted(85)
    SetObjectiveDisplayed(90)
    EnableAlias(AntonioCity)
EndFunction

Function Fragment_Stage_0216_Item_00()
    EnableAlias(AntonioCity)
EndFunction

Function Fragment_Stage_0217_Item_00()
    EnableAlias(AntonioCity)
EndFunction

Function Fragment_Stage_0218_Item_00()
    SetObjectiveCompleted(90)
    SetObjectiveDisplayed(92)
    SetObjectiveDisplayed(95)
    EnableAlias(AbbieCity)
    StartSceneIfStopped(PreAbbieConfrontation)
EndFunction

Function Fragment_Stage_0219_Item_00()
    EnableAlias(AbbieCity)
EndFunction

Function Fragment_Stage_0220_Item_00()
    SetObjectiveCompleted(92)
    StartSceneIfStopped(ConfrontationScene)
EndFunction

Function Fragment_Stage_0281_Item_00()
    Actor player = PlayerReference()
    Float ending = 0.0
    If player != None
        ending = player.GetValue(MQ_Ending_AV)
    EndIf
    If ending == 1.0
        If !IsStageDone(300)
            SetStage(300)
        EndIf
    ElseIf ending == 3.0
        If !IsStageDone(340)
            SetStage(340)
        EndIf
    ElseIf ending == 2.0
        If !IsStageDone(320)
            SetStage(320)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(95)
    Actor player = PlayerReference()
    If player != None
        player.SetValue(MQ_Ending_AV, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetObjectiveCompleted(95)
    Actor player = PlayerReference()
    If player != None
        player.SetValue(MQ_Ending_AV, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_0340_Item_00()
    SetObjectiveCompleted(95)
    Actor player = PlayerReference()
    If player != None
        player.SetValue(MQ_Ending_AV, 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0361_Item_00()
    SetObjectiveCompleted(95)
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_0364_Item_00()
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0365_Item_00()
    If !IsStageDone(320)
        SetStage(320)
    EndIf
EndFunction

Function Fragment_Stage_0367_Item_00()
    MoveActorToMarker(AbbieCity, Alias_AbbieGeneMarker)
    EnableAlias(AntonioCity)
EndFunction

Function Fragment_Stage_0369_Item_00()
    StartSceneIfStopped(GeneGoodEnding)
    RemoveAll(DevilsBloodVial)
EndFunction

Function Fragment_Stage_0370_Item_00()
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(120)
    StartSceneIfStopped(GeneBadEnding)
    RemoveAll(DevilsBloodVial)
EndFunction

Function Fragment_Stage_0380_Item_00()
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(120)
    RemoveAll(DevilsBloodVial)
EndFunction

Function Fragment_Stage_0390_Item_00()
    SetObjectiveCompleted(120)
    SetFinaleActorPresence()
    EnableAlias(Alias_Vin)
EndFunction

Function Fragment_Stage_0399_Item_00()
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(120)
    SetFinaleActorPresence()
    PrepareFinaleActors()
EndFunction

Function Fragment_Stage_0410_Item_00()
    If IsStageDone(300)
        StartSceneIfStopped(AC_MQ04_Sins_RoseRoomFinale_GoodEnding)
    ElseIf IsStageDone(340)
        StartSceneIfStopped(AC_MQ04_Sins_RoseRoomFinale_NeutralAndBadEndings)
    Else
        StartSceneIfStopped(AC_MQ04_Sins_RoseRoomFinale_NeutralAndBadEndings)
    EndIf
EndFunction

Function Fragment_Stage_0420_Item_00()
    If IsStageDone(300)
        SetObjectiveDisplayed(130)
    Else
        SetObjectiveDisplayed(140)
    EndIf
EndFunction

Function Fragment_Stage_0430_Item_00()
    SetObjectiveCompleted(130)
    SetObjectiveCompleted(140)
    If IsStageDone(300)
        SetObjectiveDisplayed(145)
    Else
        SetObjectiveDisplayed(150)
    EndIf
    StartSceneIfStopped(AC_MQ04_Sins_RoseRoomFinale_VinsEnding)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    StopSceneIfPlaying(AC_MQ04_Sins_RoseRoomFinale_GoodEnding)
    StopSceneIfPlaying(AC_MQ04_Sins_RoseRoomFinale_NeutralAndBadEndings)
    StopSceneIfPlaying(AC_MQ04_Sins_RoseRoomFinale_VinsEnding)
    AC_MQ04_Sins_QuestScript controller = QuestController()
    If controller != None
        controller.CleanupQuest()
    EndIf
EndFunction
