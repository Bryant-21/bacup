; RE_SceneKMK03 hidden stump stash. The encounter's Stump is swapped for the same-model stump container from
; ContainerSwaps (formToFind -> matchingContainer), which fills Alias_Container; the critter then leads the player to
; it. Shutdown restores the stump.
Event OnQuestInit()
    stump = Alias_Stump.GetReference()
    If stump == None || ContainerSwaps == None
        Return
    EndIf
    Int index = ContainerSwaps.FindStruct("formToFind", stump.GetBaseObject())
    If index < 0 || ContainerSwaps[index].matchingContainer == None
        Return
    EndIf
    containerRef = stump.PlaceAtMe(ContainerSwaps[index].matchingContainer)
    If containerRef == None
        Return
    EndIf
    containerRef.SetAngle(stump.GetAngleX(), stump.GetAngleY(), stump.GetAngleZ())
    stump.Disable()
    Alias_Container.ForceRefTo(containerRef)
EndEvent

Event OnQuestShutdown()
    If stump != None
        stump.Enable()
    EndIf
    If containerRef != None
        containerRef.Disable()
        containerRef.Delete()
        containerRef = None
    EndIf
EndEvent
