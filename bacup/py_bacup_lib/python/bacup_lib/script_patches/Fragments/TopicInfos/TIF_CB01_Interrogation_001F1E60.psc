Function Fragment_End(ObjectReference akSpeakerRef)
    Actor robot = akSpeakerRef as Actor
    If robot && !robot.IsDead()
        robot.StartCombat(Game.GetPlayer())
    EndIf
EndFunction
