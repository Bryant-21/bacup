Function ResetEventObjectives()
    SetObjectiveDisplayed(5, False)
    SetObjectiveCompleted(5, False)
    SetObjectiveFailed(5, False)
    SetObjectiveDisplayed(10, False)
    SetObjectiveCompleted(10, False)
    SetObjectiveFailed(10, False)
    Int index = 0
    While index < 5
        Int objective = FlowerObjective(index)
        SetObjectiveDisplayed(objective, False)
        SetObjectiveCompleted(objective, False)
        SetObjectiveFailed(objective, False)
        index += 1
    EndWhile
EndFunction

Int Function FlowerObjective(Int aiIndex)
    Return 20 + (aiIndex * 10)
EndFunction

ReferenceAlias Function FlowerAlias(Int aiIndex)
    If aiIndex == 0
        Return Alias_CorpseFlower01
    ElseIf aiIndex == 1
        Return Alias_CorpseFlower02
    ElseIf aiIndex == 2
        Return Alias_CorpseFlower03
    ElseIf aiIndex == 3
        Return Alias_CorpseFlower04
    ElseIf aiIndex == 4
        Return Alias_CorpseFlower05
    EndIf
    Return None
EndFunction

ObjectReference Function FlowerReference(Int aiIndex)
    ReferenceAlias flowerAlias = FlowerAlias(aiIndex)
    If flowerAlias == None
        Return None
    EndIf
    Return flowerAlias.GetReference()
EndFunction

FF01_DeathBlossoms_QuestScript Function EventScript()
    Return (Self as Quest) as FF01_DeathBlossoms_QuestScript
EndFunction

DefaultQuestEncounterWaveScript Function WaveSystem()
    Return (Self as Quest) as DefaultQuestEncounterWaveScript
EndFunction

Function StartCreatureWave(String asWaveID)
    DefaultQuestEncounterWaveScript waveSystem = WaveSystem()
    If waveSystem != None
        waveSystem.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StopCreatureWaves()
    DefaultQuestEncounterWaveScript waveSystem = WaveSystem()
    If waveSystem != None
        waveSystem.StopAllEncounterWaves(True)
    EndIf
EndFunction

Function ShowStartMessage()
    If FF_Small01_StartMessage == None
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    If Alias_Players != None && Alias_Players.Find(playerRef) < 0
        Return
    EndIf
    FF_Small01_StartMessage.Show()
EndFunction

Int Function PrepareFlowers()
    Int prepared = 0
    Int index = 0
    While index < 5
        ObjectReference flowerRef = FlowerReference(index)
        If flowerRef != None
            flowerRef.ClearDestruction()
            flowerRef.Enable(False)
            If Alias_Flowers != None && Alias_Flowers.Find(flowerRef) < 0
                Alias_Flowers.AddRef(flowerRef)
            EndIf
            prepared += 1
        EndIf
        index += 1
    EndWhile
    Return prepared
EndFunction

Function DisplayFlowerObjectives()
    Int index = 0
    While index < 5
        If FlowerReference(index) != None
            SetObjectiveDisplayed(FlowerObjective(index), True, True)
        EndIf
        index += 1
    EndWhile
EndFunction

Function CloseFlowerObjectives()
    Int index = 0
    While index < 5
        Int objective = FlowerObjective(index)
        If IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
            ObjectReference flowerRef = FlowerReference(index)
            If flowerRef != None && !flowerRef.IsDestroyed()
                SetObjectiveCompleted(objective, True)
            Else
                SetObjectiveFailed(objective, True)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function EmptyCreatureCollections()
    If Alias_Wave1Eaters != None
        Alias_Wave1Eaters.RemoveAll()
    EndIf
    If Alias_Wave2Eaters != None
        Alias_Wave2Eaters.RemoveAll()
    EndIf
    If Alias_Wave3Eaters != None
        Alias_Wave3Eaters.RemoveAll()
    EndIf
    If Alias_Wave4Eaters != None
        Alias_Wave4Eaters.RemoveAll()
    EndIf
EndFunction

Function EndEventWaves()
    FF01_DeathBlossoms_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StopWaveEscalation()
    EndIf
    StopCreatureWaves()
EndFunction

Function Fragment_Stage_0010_Item_00()
    ResetEventObjectives()
    SetObjectiveDisplayed(5, True, True)
    ShowStartMessage()
    If !IsStageDone(20)
        SetStage(20)
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    Int prepared = PrepareFlowers()
    FF01_DeathBlossoms_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.SetPlantsRemaining(prepared)
    ElseIf FF01_DeathBlossoms_PlantsRemaining != None
        FF01_DeathBlossoms_PlantsRemaining.SetValue(prepared as Float)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(10, True, True)
    DisplayFlowerObjectives()
    If !IsStageDone(110)
        SetStage(110)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    FF01_DeathBlossoms_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.BeginWaveEscalation()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    StartCreatureWave("Molerats")
EndFunction

Function Fragment_Stage_0300_Item_00()
    StartCreatureWave("Dogs")
EndFunction

Function Fragment_Stage_0500_Item_00()
    StartCreatureWave("Ghouls")
EndFunction

Function Fragment_Stage_1000_Item_00()
    EndEventWaves()
    SetObjectiveCompleted(10, True)
    CloseFlowerObjectives()
    If !IsStageDone(1510)
        SetStage(1510)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    EndEventWaves()
    If !IsObjectiveCompleted(10)
        SetObjectiveFailed(10, True)
    EndIf
    CloseFlowerObjectives()
    If !IsStageDone(1510)
        SetStage(1510)
    EndIf
EndFunction

Function Fragment_Stage_1510_Item_00()
    EndEventWaves()
    EmptyCreatureCollections()
    If !IsStageDone(1525)
        SetStage(1525)
    EndIf
EndFunction

Function Fragment_Stage_1525_Item_00()
    EndEventWaves()
    SetObjectiveDisplayed(5, False)
    SetObjectiveDisplayed(10, False)
    Int index = 0
    While index < 5
        SetObjectiveDisplayed(FlowerObjective(index), False)
        index += 1
    EndWhile
    If !IsStageDone(1600)
        SetStage(1600)
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    EndEventWaves()
    EmptyCreatureCollections()
    Stop()
EndFunction

Function Fragment_Stage_2000_Item_00()
    ObjectReference flowerRef = Alias_CorpseFlower01.GetReference()
    If flowerRef != None
        flowerRef.Disable(False)
    EndIf
    flowerRef = Alias_CorpseFlower02.GetReference()
    If flowerRef != None
        flowerRef.Disable(False)
    EndIf
    flowerRef = Alias_CorpseFlower03.GetReference()
    If flowerRef != None
        flowerRef.Disable(False)
    EndIf
    flowerRef = Alias_CorpseFlower04.GetReference()
    If flowerRef != None
        flowerRef.Disable(False)
    EndIf
    flowerRef = Alias_CorpseFlower05.GetReference()
    If flowerRef != None
        flowerRef.Disable(False)
    EndIf
    If Alias_Flowers != None
        Alias_Flowers.RemoveAll()
    EndIf
    If FF01_DeathBlossoms_PlantsRemaining != None
        FF01_DeathBlossoms_PlantsRemaining.SetValue(0.0)
    EndIf
EndFunction
