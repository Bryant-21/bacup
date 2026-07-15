Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        EN01_JudyGraveMessagePostDiary.Show()
    EndIf
EndEvent
