Event OnInit()
    If HazardToAdd == None
        Return
    EndIf
    spawnedHaz = PlaceAtMe(HazardToAdd, 1, true)
    If bSetAsLinkedRef && spawnedHaz
        Self.SetLinkedRef(spawnedHaz)
    EndIf
EndEvent
