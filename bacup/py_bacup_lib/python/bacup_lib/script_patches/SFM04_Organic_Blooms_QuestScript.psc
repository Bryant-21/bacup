Event OnQuestInit()
    B21StoppingBlooms = False
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    GrowLocalBlooms()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer() && IsRunning()
        GrowLocalBlooms()
    EndIf
EndEvent

Function GrowLocalBlooms()
    If !IsRunning() || B21StoppingBlooms || B21GrowingBlooms || StranglerPods == None || StranglerBlooms == None
        Return
    EndIf
    Flora bloomBase = Game.GetFormFromFile(0x0001891A, "SeventySix.esm") as Flora
    If bloomBase == None
        Return
    EndIf
    B21GrowingBlooms = True
    If B21SpawnedBlooms == None
        B21SpawnedBlooms = New ObjectReference[0]
        B21HiddenPods = New ObjectReference[0]
    EndIf
    Int groupIndex = 0
    While groupIndex < StranglerPods.Length && !B21StoppingBlooms && B21SpawnedBlooms.Length < NumPodsToBloom
        RefCollectionAlias podGroup = StranglerPods[groupIndex]
        If podGroup != None
            Int podIndex = 0
            While podIndex < podGroup.GetCount() && !B21StoppingBlooms && B21SpawnedBlooms.Length < NumPodsToBloom
                ObjectReference podRef = podGroup.GetAt(podIndex)
                If podRef != None && !podRef.IsDisabled() && B21HiddenPods.Find(podRef) < 0
                    ObjectReference bloomRef = podRef.PlaceAtMe(bloomBase, 1, True, True)
                    If bloomRef != None
                        If B21StoppingBlooms
                            bloomRef.Delete()
                        Else
                            B21SpawnedBlooms.Add(bloomRef)
                            B21HiddenPods.Add(podRef)
                            StranglerBlooms.AddRef(bloomRef)
                            podRef.DisableNoWait()
                            bloomRef.EnableNoWait()
                        EndIf
                    EndIf
                EndIf
                podIndex += 1
            EndWhile
        EndIf
        groupIndex += 1
    EndWhile
    B21GrowingBlooms = False
    If B21StoppingBlooms
        CleanupLocalBlooms()
    EndIf
EndFunction

Event OnQuestShutdown()
    B21StoppingBlooms = True
    UnregisterForAllEvents()
    CleanupLocalBlooms()
EndEvent

Function CleanupLocalBlooms()
    If B21GrowingBlooms
        Return
    EndIf
    Int index = 0
    If B21SpawnedBlooms != None
        While index < B21SpawnedBlooms.Length
            ObjectReference bloomRef = B21SpawnedBlooms[index]
            If bloomRef != None
                bloomRef.DisableNoWait()
                bloomRef.Delete()
            EndIf
            index += 1
        EndWhile
    EndIf
    index = 0
    If B21HiddenPods != None
        While index < B21HiddenPods.Length
            If B21HiddenPods[index] != None
                B21HiddenPods[index].EnableNoWait()
            EndIf
            index += 1
        EndWhile
    EndIf
    B21SpawnedBlooms = None
    B21HiddenPods = None
EndFunction
