Function Fragment_Phase_03_End()
    ObjectReference repairFurniture = RepairFurnitureRef.GetReference()
    If repairFurniture != None
        repairFurniture.Disable()
    EndIf

    Actor scavengerActor = ScavengerRef.GetActorReference()
    If scavengerActor != None
        scavengerActor.EvaluatePackage()
    EndIf
EndFunction
