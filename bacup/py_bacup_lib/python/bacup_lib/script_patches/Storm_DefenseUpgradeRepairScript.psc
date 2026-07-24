Event OnActivate(ObjectReference akActionRef)
    Actor activatingActor = akActionRef as Actor
    If activatingActor == None
        Return
    EndIf
    If SpawnedObject != None
        PlaceAtMe(SpawnedObject)
    EndIf
    If Storm_DefenseEventMessageUpgrade != None
        Storm_DefenseEventMessageUpgrade.Show()
    EndIf
    Disable()
    Delete()
EndEvent
