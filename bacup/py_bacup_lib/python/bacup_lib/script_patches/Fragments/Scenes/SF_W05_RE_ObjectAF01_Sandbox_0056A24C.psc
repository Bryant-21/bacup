Function Fragment_Phase_01_Begin()
    Actor scavengerActor = ScavengerRef.GetActorReference()
    ObjectReference repairFurniture = RepairFurnitureRef.GetReference()
    If scavengerActor != None && repairFurniture != None
        scavengerActor.SnapIntoInteraction(repairFurniture)
    EndIf
EndFunction
