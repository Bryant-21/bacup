Function PlayDustEffects()
    ObjectReference[] MyDustMarkers = Self.GetRefsLinkedToMe(LC101DustFXKeyword, None)
    Int count = 0
    While count < MyDustMarkers.Length
        Utility.Wait(Utility.RandomFloat(0.0, 0.10000000149011612))
        If Game.GetPlayer().GetDistance(MyDustMarkers[count]) < 1024.0
            Game.ShakeCamera(MyDustMarkers[count], 0.25, 3.0)
            Game.ShakeController(0.25, 0.25, 3.0)
        EndIf
        If !MyDustMarkers[count].PlayAnimation("stage2")
            MyDustMarkers[count].PlayAnimation("Reset")
        EndIf
        count += 1
    EndWhile
EndFunction
