Function HandleStage(Int aiStage)
    Quest fleeQuest = Game.GetFormFromFile(0x002D0F68, "SeventySix.esm") as Quest
    EN07_FleeSiloScript fleeScript = fleeQuest as EN07_FleeSiloScript
    If fleeScript != None
        fleeScript.HandleStage(aiStage)
    EndIf
EndFunction

Function SetCollectionEnabled(RefCollectionAlias akCollection, Bool abEnabled)
    Int i = 0
    While akCollection != None && i < akCollection.GetCount()
        ObjectReference targetRef = akCollection.GetAt(i)
        If targetRef != None
            If abEnabled
                targetRef.Enable()
            Else
                targetRef.Disable()
            EndIf
        EndIf
        i += 1
    EndWhile
EndFunction

Function Fragment_Stage_0010_Item_00()
    Alias_EnableMarker.GetReference().Enable()
    SetCollectionEnabled(Alias_Klaxons, True)
    HandleStage(10)
EndFunction

Function Fragment_Stage_0020_Item_00()
    HandleStage(20)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetCollectionEnabled(Alias_AudioMarkers, True)
    HandleStage(30)
EndFunction

Function Fragment_Stage_0035_Item_00()
    HandleStage(35)
    EN07_MQ_FleeSilo_0035_LaunchProceedureComplete.Start()
EndFunction

Function Fragment_Stage_0040_Item_00()
    HandleStage(40)
EndFunction

Function Fragment_Stage_0045_Item_00()
    HandleStage(45)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetCollectionEnabled(Alias_Klaxons, False)
    SetCollectionEnabled(Alias_AudioMarkers, False)
    HandleStage(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    HandleStage(300)
    Alias_EnableMarker.GetReference().Disable()
    EN07_MQ_FleeSilo_0300_SiloReset.Start()
EndFunction
