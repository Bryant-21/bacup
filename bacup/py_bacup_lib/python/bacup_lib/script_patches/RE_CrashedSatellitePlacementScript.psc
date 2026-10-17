; RE_ObjectKMK02 / RE_ObjectMP03 crashed satellite: the satellite is sunk zOffset into the ground and tilted by a
; random X/Y within its min/max rotation (random sign), and each scorch decal is scattered up to maxOffset around it.
Event OnQuestInit()
    ObjectReference satellite = Alias_Satellite.GetReference()
    If satellite == None
        Return
    EndIf
    Float xAngle = Utility.RandomFloat(xMinRotation, xMaxRotation)
    Float yAngle = Utility.RandomFloat(yMinRotation, yMaxRotation)
    If Utility.RandomInt(0, 1) == 0
        xAngle = -xAngle
    EndIf
    If Utility.RandomInt(0, 1) == 0
        yAngle = -yAngle
    EndIf
    satellite.SetPosition(satellite.GetPositionX(), satellite.GetPositionY(), satellite.GetPositionZ() - zOffset)
    satellite.SetAngle(xAngle, yAngle, satellite.GetAngleZ())
    Int index = Alias_Decals.GetCount() - 1
    While index >= 0
        ObjectReference decal = Alias_Decals.GetAt(index)
        If decal != None
            Float dx = Utility.RandomFloat(-maxOffset, maxOffset)
            Float dy = Utility.RandomFloat(-maxOffset, maxOffset)
            decal.SetPosition(satellite.GetPositionX() + dx, satellite.GetPositionY() + dy, decal.GetPositionZ())
        EndIf
        index -= 1
    EndWhile
EndEvent
