; The legendary vendor is set up at the SetupData slot chosen by EnableVar (Update01_Quest_LGV01 = 1 after the
; Wastelanders move): she is moved onto its placement marker and linked to its idle marker.
Event OnQuestInit()
    Actor vendor = Alias_Vendor.GetActorReference()
    If vendor == None || EnableVar == None || SetupData == None
        Return
    EndIf
    Int slot = EnableVar.GetValueInt()
    If slot < 0 || slot >= SetupData.Length || SetupData[slot].PlacementMarker == None
        Return
    EndIf
    vendor.MoveTo(SetupData[slot].PlacementMarker)
    If SetupData[slot].VendorIdleMarker != None
        vendor.SetLinkedRef(SetupData[slot].VendorIdleMarker)
    EndIf
    vendor.EvaluatePackage()
EndEvent
