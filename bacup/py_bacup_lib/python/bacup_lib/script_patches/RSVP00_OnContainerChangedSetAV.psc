Event OnEquipped(Actor akActor)
    If akActor != Game.GetPlayer()
        Return
    EndIf

    akActor.SetValue(AVToSet, SetToValue as Float)
EndEvent
