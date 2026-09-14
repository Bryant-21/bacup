Function Fragment_Stage_0010_Item_00()
    B21:LocalEncounterMaterializer materializer = (Self as Quest) as B21:LocalEncounterMaterializer
    If materializer != None
        materializer.PrepareEligibleWaves()
    EndIf
    DefaultQuestEncounterWaveScript encounterWaves = (Self as Quest) as DefaultQuestEncounterWaveScript
    If encounterWaves != None
        encounterWaves.StartLocalEncounterWave(0)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Stop()
EndFunction
